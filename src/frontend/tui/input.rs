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

/// Find the topmost window at the given screen coordinates.
/// Ephemeral windows (container discovery) have higher z-order and are checked first.
/// Returns the window name, defaulting to "main" if no window contains the point.
fn find_topmost_window_at(app_core: &crate::core::AppCore, x: u16, y: u16) -> String {
    // First check ephemeral windows (they're rendered on top)
    for window_name in &app_core.ui_state.ephemeral_windows {
        if let Some(window) = app_core.ui_state.windows.get(window_name) {
            if !window.visible {
                continue;
            }
            let pos = &window.position;
            if x >= pos.x && x < pos.x + pos.width && y >= pos.y && y < pos.y + pos.height {
                return window_name.clone();
            }
        }
    }

    // Then check regular windows
    for (name, window) in &app_core.ui_state.windows {
        if !window.visible || app_core.ui_state.ephemeral_windows.contains(name) {
            continue;
        }
        let pos = &window.position;
        if x >= pos.x && x < pos.x + pos.width && y >= pos.y && y < pos.y + pos.height {
            return name.clone();
        }
    }

    // Default to main window
    "main".to_string()
}

// TUI-specific methods (not part of Frontend trait)
impl TuiFrontend {
    pub(super) fn open_quickbar_switcher(
        &mut self,
        app_core: &mut crate::core::AppCore,
        window_pos: crate::data::WindowPosition,
    ) {
        use crate::data::ui_state::{InputMode, PopupMenu, PopupMenuItem};

        let mut ids: Vec<String> = if app_core.ui_state.quickbar_order.is_empty() {
            Vec::new()
        } else {
            app_core.ui_state.quickbar_order.clone()
        };

        ids.retain(|id| !id.trim().is_empty());

        let mut missing: Vec<String> = app_core
            .ui_state
            .quickbars
            .keys()
            .filter(|id| !ids.iter().any(|existing| existing == *id))
            .cloned()
            .collect();
        missing.sort();
        if !missing.is_empty() {
            ids.extend(missing);
        }

        if ids.is_empty() {
            let mut keys: Vec<String> = app_core.ui_state.quickbars.keys().cloned().collect();
            keys.sort();
            ids = keys;
        }

        let mut items = Vec::new();
        for id in &ids {
            let label = app_core
                .ui_state
                .quickbars
                .get(id)
                .and_then(|data| data.title.clone())
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| id.clone());
            items.push(PopupMenuItem {
                text: label,
                command: format!("_qlink change {}", id),
                disabled: false,
            });
        }

        if items.is_empty() {
            return;
        }

        let menu_height = items.len() as u16 + 2;
        let menu_x = window_pos.x;
        let menu_y = if window_pos.y >= menu_height {
            window_pos.y - menu_height
        } else {
            window_pos.y.saturating_add(1)
        };

        let mut menu = PopupMenu::new(items, (menu_x, menu_y));
        if let Some(active_id) = app_core.ui_state.active_quickbar_id.as_ref() {
            if let Some(index) = ids.iter().position(|id| id == active_id) {
                menu.selected = index;
            }
        }

        app_core.ui_state.popup_menu = Some(menu);
        app_core.ui_state.submenu = None;
        app_core.ui_state.nested_submenu = None;
        app_core.ui_state.deep_submenu = None;
        app_core.ui_state.input_mode = InputMode::Menu;
    }

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
        use crate::data::{DragOperation, DialogDragState, DialogDragOperation, LinkDragState, MouseDragState, PendingLinkClick, window::WidgetType};
        use crate::frontend::tui::dialog;
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
                use crate::frontend::tui::window_editor::WindowEditorMouseAction;

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        let action = window_editor.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        action
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        let action = window_editor.handle_mouse(*x, *y, true, area);
                        app_core.needs_render = true;
                        action
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        let action = window_editor.handle_mouse(*x, *y, false, area);
                        app_core.needs_render = true;
                        action
                    }
                    _ => WindowEditorMouseAction::None,
                };

                // Handle Save/Cancel actions from mouse clicks
                match action {
                    WindowEditorMouseAction::Save => {
                        // Trigger save via simulated Ctrl+S key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_window_editor_keys(
                            KeyCode::Char('s'),
                            KeyModifiers::CTRL,
                            app_core,
                        );
                        return Ok((true, None));
                    }
                    WindowEditorMouseAction::Cancel => {
                        // Trigger cancel via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_window_editor_keys(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                        );
                        return Ok((true, None));
                    }
                    WindowEditorMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        // Handle highlight form mouse events
        if self.highlight_form.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut form) = self.highlight_form {
                use crate::frontend::tui::highlight_form::HighlightFormMouseAction;

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area)
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area)
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, false, area)
                    }
                    _ => HighlightFormMouseAction::None,
                };

                app_core.needs_render = true;

                match action {
                    HighlightFormMouseAction::Save => {
                        // Trigger save via simulated Ctrl+S key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('s'),
                            KeyModifiers::CTRL,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    HighlightFormMouseAction::Cancel => {
                        // Trigger cancel via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    HighlightFormMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        // Handle highlight browser mouse events
        if self.highlight_browser.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut browser) = self.highlight_browser {
                use crate::frontend::tui::highlight_browser::HighlightBrowserMouseAction;

                // Determine scroll direction
                let scroll_direction: i8 = match kind {
                    MouseEventKind::ScrollUp => -1,
                    MouseEventKind::ScrollDown => 1,
                    _ => 0,
                };

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        browser.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    _ => HighlightBrowserMouseAction::None,
                };

                app_core.needs_render = true;

                match action {
                    HighlightBrowserMouseAction::Edit => {
                        // Trigger edit via simulated Enter key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Enter,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    HighlightBrowserMouseAction::Delete => {
                        // Trigger delete via simulated Delete key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Delete,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    HighlightBrowserMouseAction::Add => {
                        // Trigger add via simulated 'a' key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('a'),
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    HighlightBrowserMouseAction::Close => {
                        // Trigger close via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    HighlightBrowserMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        // Handle keybind browser mouse events
        if self.keybind_browser.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut browser) = self.keybind_browser {
                use crate::frontend::tui::keybind_browser::KeybindBrowserMouseAction;

                // Determine scroll direction
                let scroll_direction: i8 = match kind {
                    MouseEventKind::ScrollUp => -1,
                    MouseEventKind::ScrollDown => 1,
                    _ => 0,
                };

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        browser.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    _ => KeybindBrowserMouseAction::None,
                };

                app_core.needs_render = true;

                match action {
                    KeybindBrowserMouseAction::Edit => {
                        // Trigger edit via simulated Enter key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Enter,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    KeybindBrowserMouseAction::Delete => {
                        // Trigger delete via simulated Delete key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Delete,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    KeybindBrowserMouseAction::Add => {
                        // Trigger add via simulated 'a' key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('a'),
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    KeybindBrowserMouseAction::Close => {
                        // Trigger close via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    KeybindBrowserMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        // Handle keybind form mouse events
        if self.keybind_form.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut form) = self.keybind_form {
                use crate::frontend::tui::keybind_form::KeybindFormMouseAction;

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area)
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area)
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, false, area)
                    }
                    _ => KeybindFormMouseAction::None,
                };

                app_core.needs_render = true;

                match action {
                    KeybindFormMouseAction::Save => {
                        // Trigger save via simulated Ctrl+S key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('s'),
                            KeyModifiers::CTRL,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    KeybindFormMouseAction::Delete => {
                        // Trigger delete via simulated Ctrl+D key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('d'),
                            KeyModifiers::CTRL,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    KeybindFormMouseAction::Cancel => {
                        // Trigger cancel via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    KeybindFormMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        // Handle color palette browser mouse events
        if self.color_palette_browser.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut browser) = self.color_palette_browser {
                use crate::frontend::tui::color_palette_browser::ColorPaletteBrowserMouseAction;

                // Determine scroll direction
                let scroll_direction: i8 = match kind {
                    MouseEventKind::ScrollUp => -1,
                    MouseEventKind::ScrollDown => 1,
                    _ => 0,
                };

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        browser.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        browser.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    _ => ColorPaletteBrowserMouseAction::None,
                };

                app_core.needs_render = true;

                match action {
                    ColorPaletteBrowserMouseAction::Edit => {
                        // Trigger edit via simulated Enter key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Enter,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    ColorPaletteBrowserMouseAction::Delete => {
                        // Trigger delete via simulated Delete key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Delete,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    ColorPaletteBrowserMouseAction::Add => {
                        // Trigger add via simulated 'a' key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('a'),
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    ColorPaletteBrowserMouseAction::ToggleFavorite => {
                        // Favorite already toggled in handle_mouse, just trigger save via 'f' key
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('f'),
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    ColorPaletteBrowserMouseAction::Close => {
                        // Trigger close via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    ColorPaletteBrowserMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        // Handle color form mouse events
        if self.color_form.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut form) = self.color_form {
                use crate::frontend::tui::color_form::ColorFormMouseAction;

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area)
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, true, area)
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        form.handle_mouse(*x, *y, false, area)
                    }
                    _ => ColorFormMouseAction::None,
                };

                app_core.needs_render = true;

                match action {
                    ColorFormMouseAction::Save => {
                        // Trigger save via simulated Ctrl+S key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('s'),
                            KeyModifiers::CTRL,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    ColorFormMouseAction::Cancel => {
                        // Trigger cancel via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    ColorFormMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        // Handle settings editor mouse events
        if self.settings_editor.is_some() {
            let (width, height) = self.size();
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width,
                height,
            };

            if let Some(ref mut editor) = self.settings_editor {
                use crate::frontend::tui::settings_editor::SettingsEditorMouseAction;

                // Determine scroll direction
                let scroll_direction: i8 = match kind {
                    MouseEventKind::ScrollUp => -1,
                    MouseEventKind::ScrollDown => 1,
                    _ => 0,
                };

                let action = match kind {
                    MouseEventKind::Down(crate::frontend::MouseButton::Left) => {
                        editor.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                        editor.handle_mouse(*x, *y, true, scroll_direction, area)
                    }
                    MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                        editor.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        editor.handle_mouse(*x, *y, false, scroll_direction, area)
                    }
                    _ => SettingsEditorMouseAction::None,
                };

                app_core.needs_render = true;

                match action {
                    SettingsEditorMouseAction::EditValue => {
                        // Trigger edit via simulated Enter key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Enter,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    SettingsEditorMouseAction::ToggleScope => {
                        // Trigger scope toggle via simulated 'g' key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Char('g'),
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    SettingsEditorMouseAction::Close => {
                        // Trigger close via simulated Esc key press
                        use crate::frontend::KeyCode;
                        use crate::frontend::KeyModifiers;
                        let _ = self.handle_key_event(
                            KeyCode::Esc,
                            KeyModifiers::NONE,
                            app_core,
                            &handle_menu_action_fn,
                        );
                        return Ok((true, None));
                    }
                    SettingsEditorMouseAction::SelectRow | SettingsEditorMouseAction::None => {
                        return Ok((true, None));
                    }
                }
            }
        }

        if app_core.ui_state.input_mode == InputMode::Dialog {
            let (term_width, term_height) = self.size();
            let screen_area = Rect {
                x: 0,
                y: 0,
                width: term_width,
                height: term_height,
            };

            let mut command_to_send: Option<String> = None;
            let mut close_dialog = false;

            // Handle drag operations first
            match kind {
                MouseEventKind::Drag(crate::frontend::MouseButton::Left) => {
                    if let Some(ref drag_state) = app_core.ui_state.dialog_drag {
                        if let Some(ref mut dialog) = app_core.ui_state.active_dialog {
                            let dx = *x as i32 - drag_state.start_pos.0 as i32;
                            let dy = *y as i32 - drag_state.start_pos.1 as i32;

                            let min_width: u16 = 10;
                            let min_height: u16 = 4;

                            match drag_state.operation {
                                DialogDragOperation::Move => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx).max(0) as u16;
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy).max(0) as u16;

                                    // Get current dialog size to clamp position
                                    let dialog_size = dialog.size.unwrap_or(drag_state.original_dialog_size);
                                    let max_x = term_width.saturating_sub(dialog_size.0);
                                    let max_y = term_height.saturating_sub(dialog_size.1);

                                    dialog.position = Some((new_x.min(max_x), new_y.min(max_y)));
                                }
                                DialogDragOperation::ResizeRight => {
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + dx).max(min_width as i32) as u16;
                                    let max_width = term_width.saturating_sub(drag_state.original_dialog_pos.0);
                                    dialog.size = Some((new_width.min(max_width), drag_state.original_dialog_size.1));
                                }
                                DialogDragOperation::ResizeBottom => {
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + dy).max(min_height as i32) as u16;
                                    let max_height = term_height.saturating_sub(drag_state.original_dialog_pos.1);
                                    dialog.size = Some((drag_state.original_dialog_size.0, new_height.min(max_height)));
                                }
                                DialogDragOperation::ResizeBottomRight => {
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + dx).max(min_width as i32) as u16;
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + dy).max(min_height as i32) as u16;
                                    let max_width = term_width.saturating_sub(drag_state.original_dialog_pos.0);
                                    let max_height = term_height.saturating_sub(drag_state.original_dialog_pos.1);
                                    dialog.size = Some((new_width.min(max_width), new_height.min(max_height)));
                                }
                                DialogDragOperation::ResizeLeft => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx).max(0) as u16;
                                    let width_delta = drag_state.original_dialog_pos.0 as i32 - new_x as i32;
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + width_delta).max(min_width as i32) as u16;
                                    if new_width >= min_width {
                                        dialog.position = Some((new_x, drag_state.original_dialog_pos.1));
                                        dialog.size = Some((new_width, drag_state.original_dialog_size.1));
                                    }
                                }
                                DialogDragOperation::ResizeTop => {
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy).max(0) as u16;
                                    let height_delta = drag_state.original_dialog_pos.1 as i32 - new_y as i32;
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + height_delta).max(min_height as i32) as u16;
                                    if new_height >= min_height {
                                        dialog.position = Some((drag_state.original_dialog_pos.0, new_y));
                                        dialog.size = Some((drag_state.original_dialog_size.0, new_height));
                                    }
                                }
                                DialogDragOperation::ResizeTopLeft => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx).max(0) as u16;
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy).max(0) as u16;
                                    let width_delta = drag_state.original_dialog_pos.0 as i32 - new_x as i32;
                                    let height_delta = drag_state.original_dialog_pos.1 as i32 - new_y as i32;
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + width_delta).max(min_width as i32) as u16;
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + height_delta).max(min_height as i32) as u16;
                                    if new_width >= min_width && new_height >= min_height {
                                        dialog.position = Some((new_x, new_y));
                                        dialog.size = Some((new_width, new_height));
                                    }
                                }
                                DialogDragOperation::ResizeTopRight => {
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy).max(0) as u16;
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + dx).max(min_width as i32) as u16;
                                    let height_delta = drag_state.original_dialog_pos.1 as i32 - new_y as i32;
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + height_delta).max(min_height as i32) as u16;
                                    let max_width = term_width.saturating_sub(drag_state.original_dialog_pos.0);
                                    if new_height >= min_height {
                                        dialog.position = Some((drag_state.original_dialog_pos.0, new_y));
                                        dialog.size = Some((new_width.min(max_width), new_height));
                                    }
                                }
                                DialogDragOperation::ResizeBottomLeft => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx).max(0) as u16;
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + dy).max(min_height as i32) as u16;
                                    let width_delta = drag_state.original_dialog_pos.0 as i32 - new_x as i32;
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + width_delta).max(min_width as i32) as u16;
                                    let max_height = term_height.saturating_sub(drag_state.original_dialog_pos.1);
                                    if new_width >= min_width {
                                        dialog.position = Some((new_x, drag_state.original_dialog_pos.1));
                                        dialog.size = Some((new_width, new_height.min(max_height)));
                                    }
                                }
                            }
                            app_core.needs_render = true;
                        }
                    }
                    return Ok((true, None));
                }
                MouseEventKind::Up(crate::frontend::MouseButton::Left) => {
                    if app_core.ui_state.dialog_drag.is_some() {
                        // Save position if dialog has save_position flag
                        if let Some(ref dialog) = app_core.ui_state.active_dialog {
                            if dialog.save_position {
                                if let Some((x, y)) = dialog.position {
                                    use crate::config::{Config, DialogPosition};
                                    let pos = DialogPosition {
                                        x,
                                        y,
                                        width: dialog.size.map(|(w, _)| w),
                                        height: dialog.size.map(|(_, h)| h),
                                    };
                                    app_core.saved_dialog_positions.dialogs.insert(dialog.id.clone(), pos);
                                    // Save to disk asynchronously (best-effort)
                                    let character = app_core.config.character.clone();
                                    let positions = app_core.saved_dialog_positions.clone();
                                    std::thread::spawn(move || {
                                        if let Err(e) = Config::save_dialog_positions(character.as_deref(), &positions) {
                                            tracing::warn!("Failed to save dialog positions: {}", e);
                                        }
                                    });
                                }
                            }
                        }
                        app_core.ui_state.dialog_drag = None;
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                }
                _ => {}
            }

            {
                let Some(dialog_state) = app_core.ui_state.active_dialog.as_mut() else {
                    app_core.ui_state.input_mode = InputMode::Normal;
                    app_core.needs_render = true;
                    return Ok((true, None));
                };

                if let MouseEventKind::Down(crate::frontend::MouseButton::Left) = kind {
                    let layout = dialog::compute_dialog_layout(screen_area, dialog_state);

                    // Check resize handles first
                    if let Some(resize_op) = dialog::hit_test_resize_handle(&layout, *x, *y) {
                        let current_pos = dialog_state.position.unwrap_or((layout.area.x, layout.area.y));
                        let current_size = dialog_state.size.unwrap_or((layout.area.width, layout.area.height));
                        app_core.ui_state.dialog_drag = Some(DialogDragState {
                            operation: resize_op,
                            start_pos: (*x, *y),
                            original_dialog_pos: current_pos,
                            original_dialog_size: current_size,
                        });
                        // Store the computed position/size if not already set
                        if dialog_state.position.is_none() {
                            dialog_state.position = Some((layout.area.x, layout.area.y));
                        }
                        if dialog_state.size.is_none() {
                            dialog_state.size = Some((layout.area.width, layout.area.height));
                        }
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }

                    // Check title bar for move
                    if dialog::hit_test_title_bar(&layout, *x, *y) {
                        let current_pos = dialog_state.position.unwrap_or((layout.area.x, layout.area.y));
                        let current_size = dialog_state.size.unwrap_or((layout.area.width, layout.area.height));
                        app_core.ui_state.dialog_drag = Some(DialogDragState {
                            operation: DialogDragOperation::Move,
                            start_pos: (*x, *y),
                            original_dialog_pos: current_pos,
                            original_dialog_size: current_size,
                        });
                        // Store the computed position/size if not already set
                        if dialog_state.position.is_none() {
                            dialog_state.position = Some((layout.area.x, layout.area.y));
                        }
                        if dialog_state.size.is_none() {
                            dialog_state.size = Some((layout.area.width, layout.area.height));
                        }
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }

                    // Check field clicks
                    if let Some(field_index) = dialog::hit_test_field(&layout, *x, *y) {
                        Self::set_dialog_focus(dialog_state, Some(field_index));
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }

                    // Check button clicks
                    if let Some(index) = dialog::hit_test_button(&layout, *x, *y) {
                        Self::set_dialog_focus(dialog_state, None);
                        dialog_state.selected = index;
                        let (cmd, should_close) =
                            Self::activate_dialog_button(dialog_state, index);
                        command_to_send = cmd;
                        close_dialog = should_close;
                    }
                    app_core.needs_render = true;
                }
            }

            if close_dialog {
                app_core.ui_state.active_dialog = None;
                app_core.ui_state.input_mode = InputMode::Normal;
            }

            return Ok((true, command_to_send));
        }

        match kind {
            MouseEventKind::ScrollUp => {
                // Find topmost window at mouse position (ephemeral windows have higher z-order)
                let target_window = find_topmost_window_at(app_core, *x, *y);
                self.scroll_window(&target_window, 10);
                app_core.needs_render = true;
                return Ok((true, None));
            }
            MouseEventKind::ScrollDown => {
                // Find topmost window at mouse position (ephemeral windows have higher z-order)
                let target_window = find_topmost_window_at(app_core, *x, *y);
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

                        let (screen_width, screen_height) = self.size();
                        let max_x = screen_width.saturating_sub(menu_width);
                        let max_y = screen_height.saturating_sub(menu_height);
                        let menu_x = pos.0.min(max_x);
                        let menu_y = pos.1.min(max_y);
                        let menu_area = (menu_x, menu_y, menu_width, menu_height);

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
                            app_core.ui_state.deep_submenu = None;
                            app_core.ui_state.input_mode = InputMode::Normal;

                            // Handle window close command from right-click menu
                            if let Some(window_name) = command.strip_prefix("__CLOSE_WINDOW__") {
                                // Check if it's an ephemeral window
                                if app_core.ui_state.ephemeral_windows.contains(window_name) {
                                    app_core.ui_state.remove_window(window_name);
                                    app_core.ui_state.ephemeral_windows.remove(window_name);
                                    app_core.add_system_message(&format!(
                                        "Closed container window: {}",
                                        window_name
                                    ));
                                } else {
                                    // Regular window - just hide it
                                    app_core.hide_window(window_name);
                                }
                                app_core.needs_render = true;
                                return Ok((true, None));
                            }

                            // Handle perf menu close
                            if command == "__PERF_MENU_CLOSE__" {
                                app_core.ui_state.popup_menu = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                app_core.needs_render = true;
                                return Ok((true, None));
                            }

                            // Handle performance metric toggle from right-click menu
                            if let Some(metric) = command.strip_prefix("__TOGGLE_PERF__") {
                                match metric {
                                    "fps" => app_core.config.ui.perf_show_fps = !app_core.config.ui.perf_show_fps,
                                    "frame_times" => app_core.config.ui.perf_show_frame_times = !app_core.config.ui.perf_show_frame_times,
                                    "render_times" => app_core.config.ui.perf_show_render_times = !app_core.config.ui.perf_show_render_times,
                                    "ui_times" => app_core.config.ui.perf_show_ui_times = !app_core.config.ui.perf_show_ui_times,
                                    "wrap_times" => app_core.config.ui.perf_show_wrap_times = !app_core.config.ui.perf_show_wrap_times,
                                    "net" => app_core.config.ui.perf_show_net = !app_core.config.ui.perf_show_net,
                                    "parse" => app_core.config.ui.perf_show_parse = !app_core.config.ui.perf_show_parse,
                                    "events" => app_core.config.ui.perf_show_events = !app_core.config.ui.perf_show_events,
                                    "memory" => app_core.config.ui.perf_show_memory = !app_core.config.ui.perf_show_memory,
                                    "lines" => app_core.config.ui.perf_show_lines = !app_core.config.ui.perf_show_lines,
                                    "uptime" => app_core.config.ui.perf_show_uptime = !app_core.config.ui.perf_show_uptime,
                                    "jitter" => app_core.config.ui.perf_show_jitter = !app_core.config.ui.perf_show_jitter,
                                    "frame_spikes" => app_core.config.ui.perf_show_frame_spikes = !app_core.config.ui.perf_show_frame_spikes,
                                    "event_lag" => app_core.config.ui.perf_show_event_lag = !app_core.config.ui.perf_show_event_lag,
                                    "memory_delta" => app_core.config.ui.perf_show_memory_delta = !app_core.config.ui.perf_show_memory_delta,
                                    _ => {}
                                }
                                // Re-apply enabled flags to perf_stats collector
                                let data = app_core.perf_overlay_data(true);
                                app_core.perf_stats.apply_enabled_from(&data);
                                // Rebuild menu with updated checkmarks (keep it open)
                                if let Some(ref mut menu) = app_core.ui_state.popup_menu {
                                    menu.items = Self::build_perf_metrics_context_menu(&app_core.config.ui);
                                    // Keep selection in bounds
                                    if menu.selected >= menu.items.len() {
                                        menu.selected = menu.items.len().saturating_sub(1);
                                    }
                                }
                                app_core.needs_render = true;
                                return Ok((true, None));
                            }

                            // Check if this is an internal action or game command
                            if command.starts_with("action:") {
                                // Internal action - handle it
                                if let Err(e) = handle_menu_action_fn(app_core, self, &command) {
                                    tracing::error!("Menu action error: {}", e);
                                }
                                app_core.needs_render = true;
                                return Ok((true, None));
                            } else if command.starts_with(".") {
                                // Dot command - close menu and process through normal dot command handler
                                app_core.ui_state.popup_menu = None;
                                app_core.ui_state.submenu = None;
                                app_core.ui_state.nested_submenu = None;
                                app_core.ui_state.deep_submenu = None;
                                app_core.ui_state.input_mode = InputMode::Normal;
                                // Process the dot command (e.g., .menu, .help)
                                if let Err(e) = app_core.send_command(command.to_string()) {
                                    tracing::error!("Dot command error: {}", e);
                                }
                                app_core.needs_render = true;
                                return Ok((true, None));
                            } else {
                                if let Some(id) = command.strip_prefix("_qlink change ") {
                                    let id = id.trim();
                                    if !id.is_empty() {
                                        app_core.ui_state.active_quickbar_id = Some(id.to_string());
                                        if !app_core.ui_state.quickbar_order.contains(&id.to_string()) {
                                            app_core.ui_state.quickbar_order.push(id.to_string());
                                        }
                                    }
                                }
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
                        app_core.ui_state.deep_submenu = None;
                        app_core.ui_state.input_mode = InputMode::Normal;
                        app_core.needs_render = true;
                    }

                    // Don't process other clicks while in menu mode
                    return Ok((true, None));
                }

                // Mouse down handling (find links, start drags)
                app_core.ui_state.selection_state = None;

                let topmost_window = find_topmost_window_at(app_core, *x, *y);
                let (is_quickbar, window_pos) = app_core
                    .ui_state
                    .get_window(&topmost_window)
                    .map(|window| (window.widget_type == WidgetType::Quickbar, Some(window.position.clone())))
                    .unwrap_or((false, None));

                if is_quickbar {
                    if let Some(quickbar_widget) =
                        self.widget_manager.quickbar_widgets.get_mut(&topmost_window)
                    {
                        let window_pos = window_pos.unwrap_or(crate::data::WindowPosition {
                            x: 0,
                            y: 0,
                            width: 0,
                            height: 0,
                        });
                        let rect = Rect {
                            x: window_pos.x,
                            y: window_pos.y,
                            width: window_pos.width,
                            height: window_pos.height,
                        };
                        if let Some(action) = quickbar_widget.handle_click(*x, *y, rect) {
                            app_core.needs_render = true;
                            match action {
                                crate::frontend::tui::quickbar::QuickbarAction::OpenSwitcher => {
                                    self.open_quickbar_switcher(app_core, window_pos);
                                    app_core.needs_render = true;
                                    return Ok((true, None));
                                }
                                crate::frontend::tui::quickbar::QuickbarAction::ExecuteCommand(command) => {
                                    return Ok((true, Some(command)));
                                }
                                crate::frontend::tui::quickbar::QuickbarAction::MenuRequest { exist, noun } => {
                                    let command = app_core.request_menu(exist, noun, (*x, *y));
                                    return Ok((true, Some(command)));
                                }
                            }
                        }
                    }
                }

                let mut found_window = None;
                let mut drag_op = None;
                let mut handled_tab_click: Option<(String, usize)> = None;

                // Use topmost window for click processing (respects z-order for overlapping windows)
                let clicked_window_name = Some(topmost_window.clone());

                tracing::debug!(
                    "Mouse down at ({}, {}), topmost_window='{}'",
                    *x, *y, topmost_window
                );

                if let Some(window) = app_core.ui_state.get_window(&topmost_window) {
                    tracing::debug!(
                        "  Window pos: y={}, height={}, click_y={}, is_top_row={}",
                        window.position.y, window.position.height, *y, *y == window.position.y
                    );
                    let pos = &window.position;
                    let name = &topmost_window;

                    // Check if window is locked (affects resize handle detection)
                    let is_window_locked = app_core
                        .layout
                        .windows
                        .iter()
                        .find(|w| w.base().name == *name)
                        .is_some_and(|w| w.base().locked);

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
                        }
                    }

                    if handled_tab_click.is_none() {
                        let right_col = pos.x + pos.width - 1;
                        let bottom_row = pos.y + pos.height - 1;
                        let has_horizontal_space = pos.width > 1;
                        // Only use bottom row as resize handle if:
                        // 1. Window is NOT locked (locked windows can't be resized anyway)
                        // 2. Window has enough height (> 2) so there's content area between
                        //    top row (move) and bottom row (resize). For small widgets (height <= 2),
                        //    bottom row IS the content area.
                        let can_resize_bottom = !is_window_locked && pos.height > 2;
                        let can_resize_right = !is_window_locked;

                        if has_horizontal_space
                            && can_resize_bottom
                            && *x == right_col
                            && *y == bottom_row
                        {
                            drag_op = Some(DragOperation::ResizeBottomRight);
                            found_window = Some(name.clone());
                        } else if can_resize_right && has_horizontal_space && *x == right_col {
                            drag_op = Some(DragOperation::ResizeRight);
                            found_window = Some(name.clone());
                        } else if can_resize_bottom && *y == bottom_row {
                            drag_op = Some(DragOperation::ResizeBottom);
                            found_window = Some(name.clone());
                        } else if *y == pos.y {
                            drag_op = Some(DragOperation::Move);
                            found_window = Some(name.clone());
                        }
                    }
                }

                if let Some((win_name, new_index)) = handled_tab_click {
                    // Focus the tabbed window when clicking its tabs
                    app_core.ui_state.set_focus(Some(win_name.clone()));

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

                if let (Some(window_name), Some(operation)) = (found_window.clone(), drag_op.clone()) {
                    // Check if window is locked
                    let is_locked = app_core
                        .layout
                        .windows
                        .iter()
                        .find(|w| w.base().name == window_name)
                        .is_some_and(|w| w.base().locked);

                    // For Move operations, handle links based on modifiers and lock state:
                    // - Ctrl+click on link: ALWAYS starts link drag (regardless of lock)
                    // - Click on link + locked window: opens menu (can't move anyway)
                    // - Click on link + unlocked window: starts window move (repositioning)
                    let mut handled_as_link = false;
                    if operation == DragOperation::Move {
                        let has_ctrl = modifiers.ctrl;

                        // Check for links if Ctrl is held OR window is locked
                        if has_ctrl || is_locked {
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
                                    if has_ctrl {
                                        // Ctrl+click always starts link drag
                                        app_core.ui_state.link_drag_state =
                                            Some(LinkDragState {
                                                link_data,
                                                start_pos: (*x, *y),
                                                current_pos: (*x, *y),
                                            });
                                    } else {
                                        // Locked window without Ctrl: open menu
                                        app_core.ui_state.pending_link_click =
                                            Some(PendingLinkClick {
                                                link_data,
                                                click_pos: (*x, *y),
                                            });
                                    }
                                    handled_as_link = true;
                                }
                            }
                        }
                    }

                    // Only start window drag if not locked and not handled as link
                    if !handled_as_link && !is_locked {
                        if let Some(window) = app_core.ui_state.get_window(&window_name) {
                            let pos = &window.position;
                            app_core.ui_state.mouse_drag = Some(MouseDragState {
                                operation,
                                window_name,
                                start_pos: (*x, *y),
                                original_window_pos: (pos.x, pos.y, pos.width, pos.height),
                            });
                        }
                    }
                } else if let Some(window_name) = clicked_window_name {
                    // Check if this window should receive focus (text/tabbedtext only)
                    let should_focus = app_core
                        .ui_state
                        .get_window(&window_name)
                        .map(|w| matches!(w.widget_type, WidgetType::Text | WidgetType::TabbedText))
                        .unwrap_or(false);

                    if let Some(window) = app_core.ui_state.get_window(&window_name) {
                        let pos = &window.position;
                        let window_rect = ratatui::layout::Rect {
                            x: pos.x,
                            y: pos.y,
                            width: pos.width,
                            height: pos.height,
                        };

                        tracing::debug!(
                            "Non-drag click on '{}' at ({}, {}), window_rect: y={}, height={}",
                            window_name, *x, *y, window_rect.y, window_rect.height
                        );

                        if let Some(link_data) =
                            self.link_at_position(&window_name, *x, *y, window_rect)
                        {
                            tracing::debug!("  Found link: {}", link_data.noun);
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
                                        window_name.clone(),
                                    ));
                            }
                        }
                    }

                    // Apply focus after borrow on windows ends
                    if should_focus {
                        app_core.ui_state.set_focus(Some(window_name));
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

                                let window_rect = ratatui::layout::Rect {
                                    x: pos.x,
                                    y: pos.y,
                                    width: pos.width,
                                    height: pos.height,
                                };

                                // Inventory window: check link first, fallback to wear
                                if matches!(window.content, crate::data::WindowContent::Inventory(_)) {
                                    if let Some(target_link) = self.link_at_position(name, *x, *y, window_rect) {
                                        drop_target_id = Some(target_link.exist_id);
                                    } else {
                                        drop_target_hand = Some("wear".to_string());
                                    }
                                    break;
                                }

                                // Container windows: check link first, fallback to container
                                if let crate::data::WindowContent::Container { ref container_title } = window.content {
                                    // First: try to find a link at the drop position (nested container)
                                    if let Some(target_link) = self.link_at_position(name, *x, *y, window_rect) {
                                        drop_target_id = Some(target_link.exist_id);
                                    } else {
                                        // Fallback: use the window's container ID
                                        if let Some(container_data) = app_core.game_state.container_cache.find_by_title(container_title) {
                                            drop_target_id = Some(container_data.id.clone());
                                        }
                                    }
                                    break;  // Container window handled
                                }

                                // Otherwise check if we dropped on a link (non-container windows)
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
                        } else if let Some(ref coord) = pending_click.link_data.coord {
                            // Link has coord field: Look up command in cmdlist and send directly
                            if let Some(ref cmdlist) = app_core.cmdlist {
                                if let Some(entry) = cmdlist.get(coord) {
                                    // Substitute placeholders in command
                                    let command = crate::cmdlist::CmdList::substitute_command(
                                        &entry.command,
                                        &pending_click.link_data.noun,
                                        &pending_click.link_data.exist_id,
                                        None,
                                    );
                                    tracing::info!(
                                        "Executing cmdlist command for '{}' (coord: {}): {}",
                                        pending_click.link_data.text,
                                        coord,
                                        command.trim()
                                    );
                                    command_to_send = Some(format!("{}\n", command));
                                } else {
                                    tracing::warn!(
                                        "Coord {} not found in cmdlist for '{}'",
                                        coord,
                                        pending_click.link_data.text
                                    );
                                }
                            } else {
                                tracing::warn!(
                                    "Cmdlist not loaded - cannot resolve coord {} for '{}'",
                                    coord,
                                    pending_click.link_data.text
                                );
                            }
                        } else {
                            // Regular <a> tag without coord: Request context menu
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

                        // Save ephemeral container window positions to widget_state.toml
                        if app_core.ui_state.ephemeral_windows.contains(&drag_state.window_name) {
                            use crate::config::{Config, DialogPosition};
                            let pos = DialogPosition {
                                x: window.position.x,
                                y: window.position.y,
                                width: Some(window.position.width),
                                height: Some(window.position.height),
                            };
                            app_core.saved_dialog_positions.containers.insert(
                                drag_state.window_name.clone(),
                                pos,
                            );
                            // Save to disk asynchronously (best-effort)
                            let character = app_core.config.character.clone();
                            let positions = app_core.saved_dialog_positions.clone();
                            std::thread::spawn(move || {
                                if let Err(e) = Config::save_dialog_positions(character.as_deref(), &positions) {
                                    tracing::warn!("Failed to save container positions: {}", e);
                                }
                            });
                            tracing::debug!("Saved ephemeral container position for '{}'", drag_state.window_name);
                        }
                    }
                }

                app_core.ui_state.mouse_drag = None;
                app_core.ui_state.selection_drag_start = None;

                // Handle text selection copy to clipboard
                if let Some(ref selection) = app_core.ui_state.selection_state {
                    let auto_copy = app_core.config.ui.selection_auto_copy;

                    if auto_copy && !selection.is_empty() {
                        // Extract text from selection using the stored window name
                        let (start, end) = selection.normalized_range();
                        let window_name = &selection.window_name;

                        if let Some(text) = self.extract_selection_text(
                            window_name, start.line, start.col, end.line, end.col,
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
                                            "Copied {} chars to clipboard from '{}'",
                                            text.len(),
                                            window_name
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
                    // Clear selection
                    if auto_copy {
                        app_core.ui_state.selection_state = None;
                    }
                    app_core.needs_render = true;
                }

                return Ok((true, command_to_send));
            }
            MouseEventKind::Down(crate::frontend::MouseButton::Right) => {
                // Right-click on performance overlay: show metrics toggle menu
                if let Some(window) = app_core.ui_state.windows.get("performance_overlay") {
                    let pos = &window.position;
                    if *x >= pos.x && *x < pos.x + pos.width
                       && *y >= pos.y && *y < pos.y + pos.height {
                        // Build performance metrics context menu
                        let items = Self::build_perf_metrics_context_menu(&app_core.config.ui);
                        app_core.ui_state.popup_menu =
                            Some(crate::data::ui_state::PopupMenu::new(items, (*x, *y + 1)));
                        app_core.ui_state.input_mode = InputMode::Menu;
                        app_core.needs_render = true;
                        return Ok((true, None));
                    }
                }

                // Right-click: show context menu for window title bars
                for (name, window) in &app_core.ui_state.windows {
                    let pos = &window.position;
                    // Check if click is on the title bar (top row of window)
                    if *y == pos.y && *x >= pos.x && *x < pos.x + pos.width {
                        // Build context menu items
                        let mut items = Vec::new();

                        // Don't allow closing the main window
                        if name != "main" && name != "command_input" {
                            items.push(crate::data::ui_state::PopupMenuItem {
                                text: "Close Window".to_string(),
                                command: format!("__CLOSE_WINDOW__{}", name),
                                disabled: false,
                            });
                        }

                        items.push(crate::data::ui_state::PopupMenuItem {
                            text: "Edit Window...".to_string(),
                            command: format!("action:editwindow:{}", name),
                            disabled: false,
                        });

                        items.push(crate::data::ui_state::PopupMenuItem {
                            text: "Open Menu".to_string(),
                            command: ".menu".to_string(),
                            disabled: false,
                        });

                        if !items.is_empty() {
                            // Position menu just below click point
                            app_core.ui_state.popup_menu =
                                Some(crate::data::ui_state::PopupMenu::new(items, (*x, *y + 1)));
                            app_core.ui_state.input_mode = InputMode::Menu;
                            app_core.needs_render = true;
                            return Ok((true, None));
                        }
                    }
                }
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
            InputMode::Dialog => {
                let result = self.handle_dialog_mode_keys(code, modifiers, app_core)?;
                return Ok(result);
            }
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
                                crate::config::Config::compile_highlight_patterns(
                                    &mut app_core.config.highlights,
                                );
                                app_core
                                    .message_processor
                                    .apply_config(app_core.config.clone());
                                // Highlights now updated in core via apply_config()
                            }
                            self.highlight_browser = None;
                            app_core.ui_state.input_mode = InputMode::Normal;
                        }
                        crate::core::menu_actions::MenuAction::Edit => {
                            if let Some(name) = browser.get_selected() {
                                if let Some(pattern) = app_core.config.highlights.get(&name) {
                                    // Default to global if unknown
                                    let is_global = browser.get_selected_is_global().unwrap_or(true);
                                    let mut form = crate::frontend::tui::highlight_form::HighlightFormWidget::new_edit(
                                        name, pattern,
                                    );
                                    form.set_scope(is_global);
                                    self.highlight_form = Some(form);
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
                                // Default to global if unknown
                                let is_global = browser.get_selected_is_global().unwrap_or(true);
                                // Delete from appropriate file based on scope
                                if let Err(e) = crate::config::Config::delete_single_highlight(
                                    &name,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.add_system_message(&format!(
                                        "Failed to delete highlight: {}",
                                        e
                                    ));
                                } else {
                                    let scope = if is_global { "global" } else { "character" };
                                    app_core.add_system_message(&format!("Highlight deleted from {} config", scope));
                                    // Update in-memory config
                                    app_core.config.highlights.remove(&name);
                                    crate::config::Config::compile_highlight_patterns(
                                        &mut app_core.config.highlights,
                                    );
                                    app_core
                                        .message_processor
                                        .apply_config(app_core.config.clone());
                                    // Refresh browser with source tracking
                                    let global = crate::config::Config::load_common_highlights().unwrap_or_default();
                                    let character = crate::config::Config::load_character_highlights_only(
                                        app_core.config.character.as_deref()
                                    ).unwrap_or_default();
                                    browser.update_items_with_source(&global, &character);
                                }
                                tracing::info!("Deleted highlight: {} (global={})", name, is_global);
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
                                        entry.is_global,
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
                                // TODO: Phase 3.4 - delete from correct file based on is_global
                                app_core.config.keybinds.remove(&key_combo);
                                app_core.rebuild_keybind_map();
                                // Reload with source tracking for proper [G]/[C] display
                                let global_keybinds = crate::config::Config::load_common_keybinds()
                                    .unwrap_or_default();
                                let character_keybinds = crate::config::Config::load_character_keybinds_only(
                                    app_core.config.connection.character.as_deref()
                                ).unwrap_or_default();
                                browser.update_items_with_source(&global_keybinds, &character_keybinds);
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
                                let is_global = browser.get_selected_is_global().unwrap_or(true);
                                self.color_form = Some(
                                    crate::frontend::tui::color_form::ColorForm::new_edit_with_scope(
                                        color,
                                        is_global,
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
                                let is_global = browser.get_selected_is_global().unwrap_or(true);

                                // Delete from appropriate file based on scope
                                if let Err(e) = crate::config::ColorConfig::delete_single_palette_color(
                                    &color_name,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    tracing::error!("Failed to delete color: {}", e);
                                }

                                // Reload colors to update in-memory state
                                if let Ok(colors) = crate::config::ColorConfig::load_with_merge(
                                    app_core.config.character.as_deref()
                                ) {
                                    app_core.config.colors = colors;
                                }

                                // Refresh browser with updated colors
                                let global_colors = crate::config::ColorConfig::load_common_colors()
                                    .map(|c| c.color_palette)
                                    .unwrap_or_default();
                                let char_colors = crate::config::ColorConfig::load_character_colors_only(
                                    app_core.config.character.as_deref()
                                )
                                    .map(|c| c.color_palette)
                                    .unwrap_or_default();
                                browser.update_items_with_source(&global_colors, &char_colors);

                                tracing::info!("Deleted color: {} ({})", color_name, if is_global { "global" } else { "character" });
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
                    // First let the editor handle input directly
                    // Convert our KeyCode/KeyModifiers to crossterm's
                    let crossterm_code = super::crossterm_bridge::to_crossterm_keycode(code);
                    let crossterm_modifiers = super::crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key_event = crossterm::event::KeyEvent::new(crossterm_code, crossterm_modifiers);
                    let handled = editor.handle_input(key_event);

                    if handled {
                        // Check if this was a value change - apply to config immediately
                        editor.apply_to_config(&mut app_core.config);
                        app_core.needs_render = true;
                        return Ok(None);
                    }

                    // Check for Ctrl+S to save all settings
                    if modifiers.ctrl && matches!(code, KeyCode::Char('s') | KeyCode::Char('S')) {
                        // Apply and save all settings with their scopes
                        editor.apply_to_config(&mut app_core.config);

                        // Get all items and save each with its scope
                        let items_to_save: Vec<_> = editor.all_items()
                            .map(|item| (item.key.clone(), item.is_global))
                            .collect();

                        let mut save_errors = Vec::new();
                        for (key, is_global) in items_to_save {
                            if let Err(e) = app_core.config.save_single_setting(
                                &key,
                                is_global,
                                app_core.config.character.as_deref(),
                            ) {
                                save_errors.push(format!("{}: {}", key, e));
                            }
                        }

                        if save_errors.is_empty() {
                            app_core.add_system_message("Settings saved");
                        } else {
                            app_core.add_system_message(&format!(
                                "Some settings failed to save: {}",
                                save_errors.join(", ")
                            ));
                        }
                        app_core.needs_render = true;
                        return Ok(None);
                    }

                    // Handle Cancel/Escape to close editor
                    if matches!(code, KeyCode::Esc) {
                        // Apply changes to in-memory config before closing
                        editor.apply_to_config(&mut app_core.config);
                        self.settings_editor = None;
                        app_core.ui_state.input_mode = InputMode::Normal;
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
                                crate::frontend::tui::highlight_form::FormResult::Save { name, mut pattern, is_global } => {
                                    if let Some(ref fg) = pattern.fg {
                                        pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                                    }
                                    if let Some(ref bg) = pattern.bg {
                                        pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                                    }
                                    // Save to appropriate file based on scope
                                    if let Err(e) = crate::config::Config::save_single_highlight(
                                        &name,
                                        &pattern,
                                        is_global,
                                        app_core.config.character.as_deref(),
                                    ) {
                                        app_core.add_system_message(&format!(
                                            "Failed to save highlight: {}",
                                            e
                                        ));
                                    } else {
                                        let scope = if is_global { "global" } else { "character" };
                                        app_core.add_system_message(&format!("Highlight saved to {} config", scope));
                                        // Update in-memory config
                                        app_core.config.highlights.insert(name.clone(), pattern);
                                        crate::config::Config::compile_highlight_patterns(
                                            &mut app_core.config.highlights,
                                        );
                                        app_core
                                            .message_processor
                                            .apply_config(app_core.config.clone());
                                        // Refresh browser with source tracking
                                        if let Some(ref mut browser) = self.highlight_browser {
                                            let global = crate::config::Config::load_common_highlights().unwrap_or_default();
                                            let character = crate::config::Config::load_character_highlights_only(
                                                app_core.config.character.as_deref()
                                            ).unwrap_or_default();
                                            browser.update_items_with_source(&global, &character);
                                        }
                                    }
                                    tracing::info!("Saved highlight: {} (global={})", name, is_global);
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
                                is_global,
                            } => {
                                // Resolve palette color names to hex codes
                                if let Some(ref fg) = pattern.fg {
                                    pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                                }
                                if let Some(ref bg) = pattern.bg {
                                    pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                                }

                                // Save to appropriate file based on scope
                                if let Err(e) = crate::config::Config::save_single_highlight(
                                    &name,
                                    &pattern,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.add_system_message(&format!(
                                        "Failed to save highlight: {}",
                                        e
                                    ));
                                } else {
                                    let scope = if is_global { "global" } else { "character" };
                                    app_core.add_system_message(&format!("Highlight saved to {} config", scope));
                                    // Update in-memory config
                                    app_core.config.highlights.insert(name.clone(), pattern);
                                    crate::config::Config::compile_highlight_patterns(
                                        &mut app_core.config.highlights,
                                    );
                                    app_core
                                        .message_processor
                                        .apply_config(app_core.config.clone());
                                    // Refresh browser with source tracking
                                    if let Some(ref mut browser) = self.highlight_browser {
                                        let global = crate::config::Config::load_common_highlights().unwrap_or_default();
                                        let character = crate::config::Config::load_character_highlights_only(
                                            app_core.config.character.as_deref()
                                        ).unwrap_or_default();
                                        browser.update_items_with_source(&global, &character);
                                    }
                                }
                                tracing::info!("Saved highlight: {} (global={})", name, is_global);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                            crate::frontend::tui::highlight_form::FormResult::Delete { name, is_global } => {
                                // Delete from appropriate file based on scope
                                if let Err(e) = crate::config::Config::delete_single_highlight(
                                    &name,
                                    is_global,
                                    app_core.config.character.as_deref(),
                                ) {
                                    app_core.add_system_message(&format!(
                                        "Failed to delete highlight: {}",
                                        e
                                    ));
                                } else {
                                    let scope = if is_global { "global" } else { "character" };
                                    app_core.add_system_message(&format!("Highlight deleted from {} config", scope));
                                    // Update in-memory config
                                    app_core.config.highlights.remove(&name);
                                    crate::config::Config::compile_highlight_patterns(
                                        &mut app_core.config.highlights,
                                    );
                                    app_core
                                        .message_processor
                                        .apply_config(app_core.config.clone());
                                    // Refresh browser with source tracking
                                    if let Some(ref mut browser) = self.highlight_browser {
                                        let global = crate::config::Config::load_common_highlights().unwrap_or_default();
                                        let character = crate::config::Config::load_character_highlights_only(
                                            app_core.config.character.as_deref()
                                        ).unwrap_or_default();
                                        browser.update_items_with_source(&global, &character);
                                    }
                                }
                                tracing::info!("Deleted highlight: {} (global={})", name, is_global);
                                self.highlight_form = None;
                                app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                    InputMode::HighlightBrowser
                                } else {
                                    InputMode::Normal
                                };
                            }
                            crate::frontend::tui::highlight_form::FormResult::Cancel => {
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
                                        mut pattern,
                                        is_global,
                                    } => {
                                        // Resolve palette color names to hex codes
                                        if let Some(ref fg) = pattern.fg {
                                            pattern.fg = Some(app_core.config.resolve_palette_color(fg));
                                        }
                                        if let Some(ref bg) = pattern.bg {
                                            pattern.bg = Some(app_core.config.resolve_palette_color(bg));
                                        }

                                        // Save to appropriate file based on scope
                                        if let Err(e) = crate::config::Config::save_single_highlight(
                                            &name,
                                            &pattern,
                                            is_global,
                                            app_core.config.character.as_deref(),
                                        ) {
                                            app_core.add_system_message(&format!(
                                                "Failed to save highlight: {}",
                                                e
                                            ));
                                        } else {
                                            let scope = if is_global { "global" } else { "character" };
                                            app_core.add_system_message(&format!("Highlight saved to {} config", scope));
                                            // Update in-memory config
                                            app_core.config.highlights.insert(name.clone(), pattern);
                                            crate::config::Config::compile_highlight_patterns(
                                                &mut app_core.config.highlights,
                                            );
                                            app_core
                                                .message_processor
                                                .apply_config(app_core.config.clone());
                                            // Refresh browser with source tracking
                                            if let Some(ref mut browser) = self.highlight_browser {
                                                let global = crate::config::Config::load_common_highlights().unwrap_or_default();
                                                let character = crate::config::Config::load_character_highlights_only(
                                                    app_core.config.character.as_deref()
                                                ).unwrap_or_default();
                                                browser.update_items_with_source(&global, &character);
                                            }
                                        }
                                        tracing::info!("Saved highlight: {} (global={})", name, is_global);
                                        self.highlight_form = None;
                                        app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                            InputMode::HighlightBrowser
                                        } else {
                                            InputMode::Normal
                                        };
                                    }
                                    crate::frontend::tui::highlight_form::FormResult::Delete { name, is_global } => {
                                        // Delete from appropriate file based on scope
                                        if let Err(e) = crate::config::Config::delete_single_highlight(
                                            &name,
                                            is_global,
                                            app_core.config.character.as_deref(),
                                        ) {
                                            app_core.add_system_message(&format!(
                                                "Failed to delete highlight: {}",
                                                e
                                            ));
                                        } else {
                                            let scope = if is_global { "global" } else { "character" };
                                            app_core.add_system_message(&format!("Highlight deleted from {} config", scope));
                                            // Update in-memory config
                                            app_core.config.highlights.remove(&name);
                                            crate::config::Config::compile_highlight_patterns(
                                                &mut app_core.config.highlights,
                                            );
                                            app_core
                                                .message_processor
                                                .apply_config(app_core.config.clone());
                                            // Refresh browser with source tracking
                                            if let Some(ref mut browser) = self.highlight_browser {
                                                let global = crate::config::Config::load_common_highlights().unwrap_or_default();
                                                let character = crate::config::Config::load_character_highlights_only(
                                                    app_core.config.character.as_deref()
                                                ).unwrap_or_default();
                                                browser.update_items_with_source(&global, &character);
                                            }
                                        }
                                        tracing::info!("Deleted highlight: {} (global={})", name, is_global);
                                        self.highlight_form = None;
                                        app_core.ui_state.input_mode = if self.highlight_browser.is_some() {
                                            InputMode::HighlightBrowser
                                        } else {
                                            InputMode::Normal
                                        };
                                    }
                                    crate::frontend::tui::highlight_form::FormResult::Cancel => {
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
                                        is_global,
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
                                        // Save to correct file based on scope
                                        if let Err(e) = crate::config::Config::save_single_keybind(
                                            &key_combo,
                                            &action,
                                            is_global,
                                            app_core.config.connection.character.as_deref(),
                                        ) {
                                            tracing::error!("Failed to save keybind to file: {}", e);
                                        }
                                        // Also update in-memory config
                                        app_core.config.keybinds.insert(key_combo.clone(), action);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved keybind: {} (global={})", key_combo, is_global);
                                    }
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Delete {
                                        key_combo,
                                        is_global,
                                    } => {
                                        // Delete from correct file based on scope
                                        if let Err(e) = crate::config::Config::delete_single_keybind(
                                            &key_combo,
                                            is_global,
                                            app_core.config.connection.character.as_deref(),
                                        ) {
                                            tracing::error!("Failed to delete keybind from file: {}", e);
                                        }
                                        // Also update in-memory config
                                        app_core.config.keybinds.remove(&key_combo);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Deleted keybind: {} (global={})", key_combo, is_global);
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
                                        is_global,
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
                                        // Save to correct file based on scope
                                        if let Err(e) = crate::config::Config::save_single_keybind(
                                            &key_combo,
                                            &action,
                                            is_global,
                                            app_core.config.connection.character.as_deref(),
                                        ) {
                                            tracing::error!("Failed to save keybind to file: {}", e);
                                        }
                                        // Also update in-memory config
                                        app_core.config.keybinds.insert(key_combo.clone(), action);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved keybind: {} (global={})", key_combo, is_global);
                                    }
                                    crate::frontend::tui::keybind_form::KeybindFormResult::Delete {
                                        key_combo,
                                        is_global,
                                    } => {
                                        // Delete from correct file based on scope
                                        if let Err(e) = crate::config::Config::delete_single_keybind(
                                            &key_combo,
                                            is_global,
                                            app_core.config.connection.character.as_deref(),
                                        ) {
                                            tracing::error!("Failed to delete keybind from file: {}", e);
                                        }
                                        // Also update in-memory config
                                        app_core.config.keybinds.remove(&key_combo);
                                        app_core.rebuild_keybind_map();
                                        self.keybind_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Deleted keybind: {} (global={})", key_combo, is_global);
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
                                        is_global,
                                    } => {
                                        // Save to appropriate file based on scope
                                        let color_with_slot = auto_assign_slot(color.clone(), &app_core.config.colors.color_palette);
                                        if let Err(e) = crate::config::ColorConfig::save_single_palette_color(
                                            &color_with_slot,
                                            is_global,
                                            app_core.config.character.as_deref(),
                                        ) {
                                            tracing::error!("Failed to save color: {}", e);
                                        }

                                        // Handle rename: delete old name if changed
                                        if let Some(ref old_name) = original_name {
                                            if old_name != &color.name {
                                                let _ = crate::config::ColorConfig::delete_single_palette_color(
                                                    old_name,
                                                    is_global,
                                                    app_core.config.character.as_deref(),
                                                );
                                            }
                                        }

                                        // Reload colors to update in-memory state
                                        if let Ok(colors) = crate::config::ColorConfig::load_with_merge(
                                            app_core.config.character.as_deref()
                                        ) {
                                            app_core.config.colors = colors;
                                        }

                                        // Refresh browser if open
                                        if let Some(ref mut browser) = self.color_palette_browser {
                                            let global_colors = crate::config::ColorConfig::load_common_colors()
                                                .map(|c| c.color_palette)
                                                .unwrap_or_default();
                                            let char_colors = crate::config::ColorConfig::load_character_colors_only(
                                                app_core.config.character.as_deref()
                                            )
                                                .map(|c| c.color_palette)
                                                .unwrap_or_default();
                                            browser.update_items_with_source(&global_colors, &char_colors);
                                        }

                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved color: {} ({})", color.name, if is_global { "global" } else { "character" });
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
                                        is_global,
                                    } => {
                                        // Save to appropriate file based on scope
                                        let color_with_slot = auto_assign_slot(color.clone(), &app_core.config.colors.color_palette);
                                        if let Err(e) = crate::config::ColorConfig::save_single_palette_color(
                                            &color_with_slot,
                                            is_global,
                                            app_core.config.character.as_deref(),
                                        ) {
                                            tracing::error!("Failed to save color: {}", e);
                                        }

                                        // Handle rename: delete old name if changed
                                        if let Some(ref old_name) = original_name {
                                            if old_name != &color.name {
                                                let _ = crate::config::ColorConfig::delete_single_palette_color(
                                                    old_name,
                                                    is_global,
                                                    app_core.config.character.as_deref(),
                                                );
                                            }
                                        }

                                        // Reload colors to update in-memory state
                                        if let Ok(colors) = crate::config::ColorConfig::load_with_merge(
                                            app_core.config.character.as_deref()
                                        ) {
                                            app_core.config.colors = colors;
                                        }

                                        // Refresh browser if open
                                        if let Some(ref mut browser) = self.color_palette_browser {
                                            let global_colors = crate::config::ColorConfig::load_common_colors()
                                                .map(|c| c.color_palette)
                                                .unwrap_or_default();
                                            let char_colors = crate::config::ColorConfig::load_character_colors_only(
                                                app_core.config.character.as_deref()
                                            )
                                                .map(|c| c.color_palette)
                                                .unwrap_or_default();
                                            browser.update_items_with_source(&global_colors, &char_colors);
                                        }

                                        self.color_form = None;
                                        app_core.ui_state.input_mode = InputMode::Normal;
                                        tracing::info!("Saved color: {} ({})", color.name, if is_global { "global" } else { "character" });
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

    /// Handle Dialog mode keyboard navigation
    fn handle_dialog_mode_keys(
        &mut self,
        code: crate::frontend::KeyCode,
        _modifiers: crate::frontend::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::ui_state::InputMode;
        use crate::frontend::KeyCode;

        let mut command_to_send: Option<String> = None;
        let mut close_dialog = false;

        {
            let Some(dialog) = app_core.ui_state.active_dialog.as_mut() else {
                app_core.ui_state.input_mode = InputMode::Normal;
                app_core.needs_render = true;
                return Ok(None);
            };

            if let Some(field_index) = dialog.focused_field {
                if field_index >= dialog.fields.len() {
                    Self::set_dialog_focus(dialog, None);
                }
            }

            if let Some(field_index) = dialog.focused_field {
                let field = &mut dialog.fields[field_index];
                match code {
                    KeyCode::Esc => close_dialog = true,
                    KeyCode::Tab => {
                        Self::move_dialog_focus(dialog, false);
                        app_core.needs_render = true;
                    }
                    KeyCode::BackTab => {
                        Self::move_dialog_focus(dialog, true);
                        app_core.needs_render = true;
                    }
                    KeyCode::Up => {
                        Self::move_dialog_focus(dialog, true);
                        app_core.needs_render = true;
                    }
                    KeyCode::Down => {
                        Self::move_dialog_focus(dialog, false);
                        app_core.needs_render = true;
                    }
                    KeyCode::Left => {
                        if field.cursor > 0 {
                            field.cursor -= 1;
                            app_core.needs_render = true;
                        }
                    }
                    KeyCode::Right => {
                        if field.cursor < field.value.len() {
                            field.cursor += 1;
                            app_core.needs_render = true;
                        }
                    }
                    KeyCode::Home => {
                        field.cursor = 0;
                        app_core.needs_render = true;
                    }
                    KeyCode::End => {
                        field.cursor = field.value.len();
                        app_core.needs_render = true;
                    }
                    KeyCode::Backspace => {
                        if field.cursor > 0 && !field.value.is_empty() {
                            let remove_at = field.cursor - 1;
                            field.value.remove(remove_at);
                            field.cursor -= 1;
                            app_core.needs_render = true;
                        }
                    }
                    KeyCode::Delete => {
                        if field.cursor < field.value.len() {
                            field.value.remove(field.cursor);
                            app_core.needs_render = true;
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(button_id) = field.enter_button.clone() {
                            if let Some(index) = Self::find_dialog_button_index(dialog, &button_id)
                            {
                                dialog.selected = index;
                                let (cmd, should_close) =
                                    Self::activate_dialog_button(dialog, index);
                                command_to_send = cmd;
                                close_dialog = should_close;
                            }
                        }
                        app_core.needs_render = true;
                    }
                    KeyCode::Char(ch) => {
                        field.value.insert(field.cursor, ch);
                        field.cursor += 1;
                        app_core.needs_render = true;
                    }
                    _ => {}
                }
            } else {
                match code {
                    KeyCode::Esc => {
                        close_dialog = true;
                    }
                    KeyCode::Tab => {
                        Self::move_dialog_focus(dialog, false);
                        app_core.needs_render = true;
                    }
                    KeyCode::BackTab => {
                        Self::move_dialog_focus(dialog, true);
                        app_core.needs_render = true;
                    }
                    KeyCode::Up | KeyCode::Left => {
                        if !dialog.buttons.is_empty() {
                            if dialog.selected == 0 {
                                dialog.selected = dialog.buttons.len() - 1;
                            } else {
                                dialog.selected -= 1;
                            }
                        }
                        app_core.needs_render = true;
                    }
                    KeyCode::Down | KeyCode::Right => {
                        if !dialog.buttons.is_empty() {
                            dialog.selected = (dialog.selected + 1) % dialog.buttons.len();
                        }
                        app_core.needs_render = true;
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if !dialog.buttons.is_empty() {
                            let (cmd, should_close) =
                                Self::activate_dialog_button(dialog, dialog.selected);
                            command_to_send = cmd;
                            close_dialog = should_close;
                        }
                        app_core.needs_render = true;
                    }
                    _ => {}
                }
            }
        }

        if close_dialog {
            app_core.ui_state.active_dialog = None;
            app_core.ui_state.input_mode = InputMode::Normal;
        }

        Ok(command_to_send)
    }

    fn dialog_command_with_placeholders(
        dialog: &crate::data::DialogState,
        command: &str,
    ) -> String {
        let mut resolved = command.to_string();
        for field in &dialog.fields {
            let token = format!("%{}%", field.id);
            resolved = resolved.replace(&token, &field.value);
        }
        resolved
    }

    fn find_dialog_button_index(dialog: &crate::data::DialogState, id: &str) -> Option<usize> {
        dialog.buttons.iter().position(|button| button.id == id)
    }

    fn set_dialog_focus(dialog: &mut crate::data::DialogState, focused: Option<usize>) {
        dialog.focused_field = focused.filter(|idx| *idx < dialog.fields.len());
        for (idx, field) in dialog.fields.iter_mut().enumerate() {
            field.focused = dialog.focused_field == Some(idx);
            if field.cursor > field.value.len() {
                field.cursor = field.value.len();
            }
        }
    }

    fn move_dialog_focus(dialog: &mut crate::data::DialogState, reverse: bool) {
        let field_count = dialog.fields.len();
        let button_count = dialog.buttons.len();
        let total = field_count + button_count;
        if total == 0 {
            return;
        }

        if dialog.focused_field.is_none() && field_count > 0 {
            if reverse {
                if button_count > 0 {
                    Self::set_dialog_focus(dialog, None);
                    dialog.selected = button_count - 1;
                } else {
                    Self::set_dialog_focus(dialog, Some(field_count - 1));
                }
            } else {
                Self::set_dialog_focus(dialog, Some(0));
            }
            return;
        }

        let current_index = if let Some(field_idx) = dialog.focused_field {
            field_idx
        } else if button_count > 0 {
            field_count + dialog.selected
        } else {
            0
        };

        let next_index = if reverse {
            (current_index + total - 1) % total
        } else {
            (current_index + 1) % total
        };

        if next_index < field_count {
            Self::set_dialog_focus(dialog, Some(next_index));
        } else {
            Self::set_dialog_focus(dialog, None);
            dialog.selected = next_index - field_count;
        }
    }

    fn activate_dialog_button(
        dialog: &mut crate::data::DialogState,
        index: usize,
    ) -> (Option<String>, bool) {
        let mut command_to_send: Option<String> = None;
        let mut close_dialog = false;

        if let Some(button) = dialog.buttons.get(index) {
            let button_id = button.id.clone();
            let button_cmd = button.command.clone();
            let button_autosend = button.autosend;
            let button_is_radio = button.is_radio;
            let button_is_close = button.is_close;
            let button_group = button.group.clone();

            if button_is_close {
                if !button_cmd.trim().is_empty() {
                    let resolved = Self::dialog_command_with_placeholders(dialog, &button_cmd);
                    command_to_send = Some(format!("{}\n", resolved));
                }
                close_dialog = true;
            } else if button_is_radio {
                for other in dialog.buttons.iter_mut() {
                    if other.is_radio && other.group == button_group {
                        other.selected = other.id == button_id;
                    }
                }
                if button_autosend {
                    let resolved = Self::dialog_command_with_placeholders(dialog, &button_cmd);
                    command_to_send = Some(format!("{}\n", resolved));
                }
            } else {
                let resolved = Self::dialog_command_with_placeholders(dialog, &button_cmd);
                command_to_send = Some(format!("{}\n", resolved));
            }
        }

        (command_to_send, close_dialog)
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
                if app_core.ui_state.deep_submenu.is_some() {
                    // Close the deepest level first (level 4)
                    app_core.ui_state.deep_submenu = None;
                } else if app_core.ui_state.nested_submenu.is_some() {
                    // Close level 3
                    app_core.ui_state.nested_submenu = None;
                } else if app_core.ui_state.submenu.is_some() {
                    // Close level 2
                    app_core.ui_state.submenu = None;
                } else {
                    // Close all menus and return to normal mode
                    app_core.ui_state.popup_menu = None;
                    app_core.ui_state.submenu = None;
                    app_core.ui_state.nested_submenu = None;
                    app_core.ui_state.deep_submenu = None;
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
                app_core.needs_render = true;
            }
            KeyCode::Tab | KeyCode::Down => {
                if let Some(ref mut deep) = app_core.ui_state.deep_submenu {
                    deep.select_next();
                } else if let Some(ref mut nested) = app_core.ui_state.nested_submenu {
                    nested.select_next();
                } else if let Some(ref mut submenu) = app_core.ui_state.submenu {
                    submenu.select_next();
                } else if let Some(ref mut menu) = app_core.ui_state.popup_menu {
                    menu.select_next();
                }
                app_core.needs_render = true;
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(ref mut deep) = app_core.ui_state.deep_submenu {
                    deep.select_prev();
                } else if let Some(ref mut nested) = app_core.ui_state.nested_submenu {
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
                let menu_to_use = if app_core.ui_state.deep_submenu.is_some() {
                    &app_core.ui_state.deep_submenu
                } else if app_core.ui_state.nested_submenu.is_some() {
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

            // Create menu at the right level based on current menu state
            if app_core.ui_state.submenu.is_some() {
                // Already have a submenu, create nested_submenu
                let parent_pos = app_core
                    .ui_state
                    .submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .unwrap_or((40, 12));
                app_core.ui_state.nested_submenu = Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            } else if app_core.ui_state.popup_menu.is_some() {
                // Have popup_menu, create submenu
                let parent_pos = app_core
                    .ui_state
                    .popup_menu
                    .as_ref()
                    .map(|m| m.get_position())
                    .unwrap_or((40, 12));
                app_core.ui_state.submenu = Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            } else {
                // No existing menu, create popup_menu
                app_core.ui_state.popup_menu = Some(PopupMenu::new(items, (40, 12)));
            }
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
                app_core.ui_state.deep_submenu = None;
            } else {
                // Category menu is at nested_submenu (level 3), so template menu goes to deep_submenu (level 4)
                let parent_pos = app_core
                    .ui_state
                    .nested_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.deep_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_INDICATORS" {
            // Indicator submenu under Status (replaces deep_submenu since we're at level 4)
            let templates = crate::config::Config::get_addable_templates_by_category(&app_core.layout, app_core.game_type())
                .get(&crate::config::WidgetCategory::Status)
                .cloned()
                .unwrap_or_default();
            let items = app_core.build_indicator_add_menu(&templates);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .deep_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.nested_submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.deep_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if let Some(category_str) = command.strip_prefix("__SUBMENU_HIDE__") {
            let category = Self::parse_widget_category(category_str, app_core)?;
            let items = app_core.build_hide_window_category_menu(&category);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                // Category menu is at nested_submenu (level 3), so template menu goes to deep_submenu (level 4)
                let parent_pos = app_core
                    .ui_state
                    .nested_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.deep_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if let Some(category_str) = command.strip_prefix("__SUBMENU_EDIT__") {
            let category = Self::parse_widget_category(category_str, app_core)?;
            let items = app_core.build_edit_window_category_menu(&category);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                // Category menu is at nested_submenu (level 3), so template menu goes to deep_submenu (level 4)
                let parent_pos = app_core
                    .ui_state
                    .nested_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.deep_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_HIDE_INDICATORS" {
            // Indicator hide submenu (replaces deep_submenu since we're at level 4)
            let indicators = app_core
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible && matches!(w.widget_type(), "indicator"))
                .map(|w| w.name().to_string())
                .collect::<Vec<String>>();
            let items = app_core.build_indicator_hide_menu(&indicators);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .deep_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.nested_submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.deep_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_EDIT_INDICATORS" {
            // Indicator edit submenu (replaces deep_submenu since we're at level 4)
            let indicators = app_core
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible && matches!(w.widget_type(), "indicator"))
                .map(|w| w.name().to_string())
                .collect::<Vec<String>>();
            let items = app_core.build_indicator_edit_menu(&indicators);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let parent_pos = app_core
                    .ui_state
                    .deep_submenu
                    .as_ref()
                    .map(|m| m.get_position())
                    .or_else(|| app_core.ui_state.nested_submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.submenu.as_ref().map(|m| m.get_position()))
                    .or_else(|| app_core.ui_state.popup_menu.as_ref().map(|m| m.get_position()))
                    .unwrap_or((40, 12));
                app_core.ui_state.deep_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            }
            app_core.needs_render = true;
        } else if command == "__INDICATOR_EDITOR" {
            self.indicator_template_editor =
                Some(crate::frontend::tui::indicator_template_editor::IndicatorTemplateEditor::new());
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.submenu = None;
            app_core.ui_state.nested_submenu = None;
            app_core.ui_state.deep_submenu = None;
            app_core.ui_state.input_mode = crate::data::ui_state::InputMode::IndicatorTemplateEditor;
            app_core.needs_render = true;
        } else if let Some(widget_type) = command.strip_prefix("__ADD_CUSTOM__") {
            // Start a new blank/custom window editor for this widget type
            // Safeguard: prevent opening if a window editor is already open
            if self.window_editor.is_some() {
                tracing::debug!("Window editor already open, ignoring add custom request");
            } else {
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
            }
        } else if let Some(window_name) = command.strip_prefix("__ADD__") {
            match app_core.layout.add_window(window_name) {
                Ok(_) => {
                    let (width, height) = self.size();
                    // Only add the NEW window to UI state, don't overwrite existing windows
                    // (sync_layout_to_ui_state was overwriting all windows, resetting user changes)

                    // For templates with auto-generated names (spacer, tabbedtext_custom, etc.)
                    // we need to get the last window in the layout since the template name
                    // differs from the actual window name (e.g., "tabbedtext_custom" → "custom-tabbedtext-1")
                    // First try direct lookup, then fallback to last window if template doesn't match
                    let window_def = app_core.layout.get_window(window_name).cloned()
                        .or_else(|| app_core.layout.windows.last().cloned());

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
            app_core.ui_state.deep_submenu = None;
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
            app_core.ui_state.deep_submenu = None;
            app_core.needs_render = true;
        } else if let Some(window_name) = command.strip_prefix("__EDIT__") {
            // Safeguard: prevent opening if a window editor is already open
            if self.window_editor.is_some() {
                tracing::debug!("Window editor already open, ignoring edit request for: {}", window_name);
            } else if let Some(window_def) = app_core.layout.get_window(window_name) {
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
            app_core.ui_state.deep_submenu = None;
            app_core.needs_render = true;
        } else if let Some(window_name) = command.strip_prefix("__CLOSE_WINDOW__") {
            // Handle window close from right-click menu
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.submenu = None;
            app_core.ui_state.nested_submenu = None;
            app_core.ui_state.deep_submenu = None;
            app_core.ui_state.input_mode = InputMode::Normal;

            // Check if it's an ephemeral window
            if app_core.ui_state.ephemeral_windows.contains(window_name) {
                app_core.ui_state.remove_window(window_name);
                app_core.ui_state.ephemeral_windows.remove(window_name);
                app_core.add_system_message(&format!("Closed container window: {}", window_name));
            } else {
                // Regular window - just hide it
                app_core.hide_window(window_name);
            }
            app_core.needs_render = true;
        } else if command == "__PERF_MENU_CLOSE__" {
            // Close the perf metrics menu
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.input_mode = InputMode::Normal;
            app_core.needs_render = true;
        } else if let Some(metric) = command.strip_prefix("__TOGGLE_PERF__") {
            // Handle performance overlay metric toggle from right-click menu
            match metric {
                "fps" => app_core.config.ui.perf_show_fps = !app_core.config.ui.perf_show_fps,
                "frame_times" => app_core.config.ui.perf_show_frame_times = !app_core.config.ui.perf_show_frame_times,
                "render_times" => app_core.config.ui.perf_show_render_times = !app_core.config.ui.perf_show_render_times,
                "ui_times" => app_core.config.ui.perf_show_ui_times = !app_core.config.ui.perf_show_ui_times,
                "wrap_times" => app_core.config.ui.perf_show_wrap_times = !app_core.config.ui.perf_show_wrap_times,
                "net" => app_core.config.ui.perf_show_net = !app_core.config.ui.perf_show_net,
                "parse" => app_core.config.ui.perf_show_parse = !app_core.config.ui.perf_show_parse,
                "events" => app_core.config.ui.perf_show_events = !app_core.config.ui.perf_show_events,
                "memory" => app_core.config.ui.perf_show_memory = !app_core.config.ui.perf_show_memory,
                "lines" => app_core.config.ui.perf_show_lines = !app_core.config.ui.perf_show_lines,
                "uptime" => app_core.config.ui.perf_show_uptime = !app_core.config.ui.perf_show_uptime,
                "jitter" => app_core.config.ui.perf_show_jitter = !app_core.config.ui.perf_show_jitter,
                "frame_spikes" => app_core.config.ui.perf_show_frame_spikes = !app_core.config.ui.perf_show_frame_spikes,
                "event_lag" => app_core.config.ui.perf_show_event_lag = !app_core.config.ui.perf_show_event_lag,
                "memory_delta" => app_core.config.ui.perf_show_memory_delta = !app_core.config.ui.perf_show_memory_delta,
                _ => {}
            }
            // Re-apply enabled flags to perf_stats collector
            let data = app_core.perf_overlay_data(true);
            app_core.perf_stats.apply_enabled_from(&data);
            // Rebuild menu with updated checkmarks (keep it open)
            if let Some(ref mut menu) = app_core.ui_state.popup_menu {
                menu.items = Self::build_perf_metrics_context_menu(&app_core.config.ui);
                // Keep selection in bounds
                if menu.selected >= menu.items.len() {
                    menu.selected = menu.items.len().saturating_sub(1);
                }
            }
            app_core.needs_render = true;
        } else {
            // Internal action commands should manage menus themselves
            if command.starts_with("action:") {
                handle_menu_action_fn(app_core, self, &command)?;
                app_core.needs_render = true;
            } else if command.starts_with(".") {
                // Dot command - close menu and process through normal dot command handler
                app_core.ui_state.popup_menu = None;
                app_core.ui_state.submenu = None;
                app_core.ui_state.nested_submenu = None;
                app_core.ui_state.deep_submenu = None;
                app_core.ui_state.input_mode = InputMode::Normal;
                // Process the dot command (e.g., .menu, .help)
                if let Err(e) = app_core.send_command(command.to_string()) {
                    tracing::error!("Dot command error: {}", e);
                }
                app_core.needs_render = true;
            } else {
                if let Some(id) = command.strip_prefix("_qlink change ") {
                    let id = id.trim();
                    if !id.is_empty() {
                        app_core.ui_state.active_quickbar_id = Some(id.to_string());
                        if !app_core.ui_state.quickbar_order.contains(&id.to_string()) {
                            app_core.ui_state.quickbar_order.push(id.to_string());
                        }
                        app_core.needs_render = true;
                    }
                }
                // Game command or empty selection: close menus
                app_core.ui_state.popup_menu = None;
                app_core.ui_state.submenu = None;
                app_core.ui_state.nested_submenu = None;
                app_core.ui_state.deep_submenu = None;
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

    /// Build performance overlay metrics context menu with checkmarks for enabled metrics
    fn build_perf_metrics_context_menu(ui: &crate::config::UiConfig) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let check = |on: bool| if on { "✓" } else { " " };
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] FPS", check(ui.perf_show_fps)),
                command: "__TOGGLE_PERF__fps".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Render Times", check(ui.perf_show_render_times)),
                command: "__TOGGLE_PERF__render_times".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] UI Times", check(ui.perf_show_ui_times)),
                command: "__TOGGLE_PERF__ui_times".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Wrap Times", check(ui.perf_show_wrap_times)),
                command: "__TOGGLE_PERF__wrap_times".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Network", check(ui.perf_show_net)),
                command: "__TOGGLE_PERF__net".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Parse Stats", check(ui.perf_show_parse)),
                command: "__TOGGLE_PERF__parse".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Events", check(ui.perf_show_events)),
                command: "__TOGGLE_PERF__events".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Memory", check(ui.perf_show_memory)),
                command: "__TOGGLE_PERF__memory".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Lines/Windows", check(ui.perf_show_lines)),
                command: "__TOGGLE_PERF__lines".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Uptime", check(ui.perf_show_uptime)),
                command: "__TOGGLE_PERF__uptime".to_string(),
                disabled: false,
            },
            // Advanced metrics (usually disabled by default)
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Frame Times", check(ui.perf_show_frame_times)),
                command: "__TOGGLE_PERF__frame_times".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Jitter", check(ui.perf_show_jitter)),
                command: "__TOGGLE_PERF__jitter".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Frame Spikes", check(ui.perf_show_frame_spikes)),
                command: "__TOGGLE_PERF__frame_spikes".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Event Lag", check(ui.perf_show_event_lag)),
                command: "__TOGGLE_PERF__event_lag".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: format!("[{}] Memory Delta", check(ui.perf_show_memory_delta)),
                command: "__TOGGLE_PERF__memory_delta".to_string(),
                disabled: false,
            },
            // Separator and Close button
            crate::data::ui_state::PopupMenuItem {
                text: "─────────────".to_string(),
                command: String::new(),
                disabled: true,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Close".to_string(),
                command: "__PERF_MENU_CLOSE__".to_string(),
                disabled: false,
            },
        ]
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
                    } else if editor.is_on_perception_sort_direction() {
                        editor.cycle_perception_sort_direction();
                        app_core.needs_render = true;
                    } else if editor.is_on_perception_short_spell_names() {
                        editor.toggle_perception_short_spell_names();
                        app_core.needs_render = true;
                    } else if editor.is_on_edit_tabs()
                        || editor.is_on_edit_indicators()
                        || editor.is_on_perception_replacements()
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
                            || editor.is_on_perception_sort_direction()
                            || editor.is_on_perception_short_spell_names()
                            || editor.is_on_perception_replacements()
                        {
                            if editor.is_on_checkbox() {
                                editor.toggle_field();
                            } else if editor.is_on_content_align() {
                                editor.cycle_content_align(false);
                            } else if editor.is_on_title_position() {
                                editor.cycle_title_position(false);
                            } else if editor.is_on_tab_bar_position() {
                                editor.cycle_tab_bar_position();
                            } else if editor.is_on_perception_sort_direction() {
                                editor.cycle_perception_sort_direction();
                            } else if editor.is_on_perception_short_spell_names() {
                                editor.toggle_perception_short_spell_names();
                            } else if editor.is_on_border_style() {
                                editor.cycle_border_style(false);
                            } else if editor.is_on_edit_tabs()
                                || editor.is_on_edit_indicators()
                                || editor.is_on_perception_replacements()
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

                                // For TabbedText windows, sync tabs and reset widget cache if structure changed
                                if matches!(window_def, crate::config::WindowDef::TabbedText { .. }) {
                                    let window_name = window_def.name().to_string();
                                    if app_core.sync_tabbed_window_tabs(&window_name) {
                                        app_core.ui_state.needs_widget_reset = true;
                                    }
                                }
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
