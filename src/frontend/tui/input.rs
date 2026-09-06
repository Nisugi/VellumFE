use super::*;
use crate::config::Config;

/// Find the next available palette slot for a new color.
/// Searches slots 16-231 (color cube range) for the first unused slot.
/// Returns None if all slots are taken.
fn find_next_available_slot(palette: &[crate::config::PaletteColor]) -> Option<u8> {
    let used_slots: std::collections::HashSet<u8> = palette.iter().filter_map(|c| c.slot).collect();

    // Search in color cube range (16-231), avoiding ANSI colors (0-15) and grayscale (232-255)
    (16u8..=231).find(|slot| !used_slots.contains(slot))
}

/// Assign the next available slot to a color if it doesn't have one
pub(super) fn auto_assign_slot(
    mut color: crate::config::PaletteColor,
    palette: &[crate::config::PaletteColor],
) -> crate::config::PaletteColor {
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
/// Topmost window whose rect contains `(x, y)`, falling back to "main".
/// Ephemeral windows render on top so they win; otherwise any visible window
/// containing the point. Takes `&UiState` (not the whole AppCore) so the pure
/// hit-test can be unit-tested without a live core.
fn find_topmost_window_at(ui_state: &crate::data::ui_state::UiState, x: u16, y: u16) -> String {
    // First check ephemeral windows (they're rendered on top)
    for window_name in &ui_state.ephemeral_windows {
        if let Some(window) = ui_state.windows.get(window_name) {
            if !window.visible {
                continue;
            }
            let pos = &window.position;
            if x >= pos.x.get()
                && x < pos.x.get() + pos.width.get()
                && y >= pos.y.get()
                && y < pos.y.get() + pos.height.get()
            {
                return window_name.clone();
            }
        }
    }

    // Then check regular windows
    for (name, window) in &ui_state.windows {
        if !window.visible || ui_state.ephemeral_windows.contains(name) {
            continue;
        }
        let pos = &window.position;
        if x >= pos.x.get()
            && x < pos.x.get() + pos.width.get()
            && y >= pos.y.get()
            && y < pos.y.get() + pos.height.get()
        {
            return name.clone();
        }
    }

    // Default to main window
    "main".to_string()
}

/// Which drag operation a mouse-down at `(x, y)` starts on a window occupying
/// `(win_x, win_y, width, height)`, or None for a plain body click. Pure edge
/// geometry, matching the TUI's border-as-handle convention:
/// - top row → Move
/// - right column → ResizeRight (and the bottom-right cell → ResizeBottomRight)
/// - bottom row → ResizeBottom
/// A locked window resizes nowhere (only Move via the top row). The bottom row
/// only acts as a resize handle when height > 2, so a 1-2 row widget's bottom
/// row stays content, not a handle. Right needs width > 1 for the same reason.
fn resize_op_at(
    win_x: u16,
    win_y: u16,
    width: u16,
    height: u16,
    locked: bool,
    x: u16,
    y: u16,
) -> Option<crate::data::DragOperation> {
    use crate::data::DragOperation;
    let right_col = win_x + width - 1;
    let bottom_row = win_y + height - 1;
    let has_horizontal_space = width > 1;
    let can_resize_bottom = !locked && height > 2;
    let can_resize_right = !locked;

    if has_horizontal_space && can_resize_bottom && x == right_col && y == bottom_row {
        Some(DragOperation::ResizeBottomRight)
    } else if can_resize_right && has_horizontal_space && x == right_col {
        Some(DragOperation::ResizeRight)
    } else if can_resize_bottom && y == bottom_row {
        Some(DragOperation::ResizeBottom)
    } else if y == win_y {
        Some(DragOperation::Move)
    } else {
        None
    }
}

// TUI-specific methods (not part of Frontend trait)
impl TuiFrontend {
    /// Write the in-memory color config to the profile colors.toml, matching
    /// the GUI colors editor's persist path. Without this, spell-color and UI
    /// color edits lived only in memory and vanished on restart or
    /// `.reload colors`.
    pub(super) fn persist_colors(app_core: &mut crate::core::AppCore) {
        let character = app_core.config.character.clone();
        if let Err(err) = app_core.config.colors.save(character.as_deref()) {
            tracing::error!("Failed to save colors: {}", err);
            app_core.add_system_message(&format!("Failed to save colors: {}", err));
        }
    }

    /// Apply a spell-color form result to the config and persist. Single
    /// source for both spell-color-form dispatch paths (action-routed and
    /// raw-key), so Save/Delete behave identically and can't drift. Save
    /// resolves palette names to hex, then REPLACES the edited entry in place
    /// (`edit_index`) or appends a new one — the old code always pushed, so
    /// editing a range left a duplicate behind.
    pub(super) fn handle_spell_color_form_result(
        &mut self,
        app_core: &mut crate::core::AppCore,
        result: crate::frontend::tui::spell_color_form::SpellColorFormResult,
    ) {
        use crate::frontend::tui::spell_color_form::SpellColorFormResult;
        match result {
            SpellColorFormResult::Save {
                mut range,
                edit_index,
            } => {
                // Resolve palette color names to hex codes.
                range.color = app_core.config.resolve_palette_color(&range.color);
                if let Some(ref bar) = range.bar_color {
                    range.bar_color = Some(app_core.config.resolve_palette_color(bar));
                }
                if let Some(ref text) = range.text_color {
                    range.text_color = Some(app_core.config.resolve_palette_color(text));
                }
                if let Some(ref bg) = range.bg_color {
                    range.bg_color = Some(app_core.config.resolve_palette_color(bg));
                }

                let spell_colors = &mut app_core.config.colors.spell_colors;
                match edit_index {
                    Some(index) if index < spell_colors.len() => {
                        spell_colors[index] = range;
                        tracing::info!("Updated spell color range {}", index);
                    }
                    // Missing/out-of-range edit index falls back to append so
                    // the user's edit is never silently lost.
                    _ => {
                        spell_colors.push(range);
                        tracing::info!("Saved spell color range");
                    }
                }
                Self::persist_colors(app_core);
            }
            SpellColorFormResult::Delete(index) => {
                if index < app_core.config.colors.spell_colors.len() {
                    app_core.config.colors.spell_colors.remove(index);
                    Self::persist_colors(app_core);
                    tracing::info!("Deleted spell color range");
                }
            }
            SpellColorFormResult::Cancel => {}
        }
        self.spell_color_form = None;
        app_core.ui_state.input_mode = crate::data::ui_state::InputMode::Normal;
    }

    /// Apply an inline `.uicolors` edit back onto the color config. Entries
    /// are addressed the same way the browser builds them: category "UI"
    /// (named UiColors fields), "PRESETS" (preset map key), or "PROMPT"
    /// ("Prompt (c)" per prompt character).
    pub(super) fn apply_ui_color_edit(
        app_core: &mut crate::core::AppCore,
        category: &str,
        name: &str,
        fg: Option<String>,
        bg: Option<String>,
    ) {
        let colors = &mut app_core.config.colors;
        match category {
            "UI" => {
                // UiColors fields are plain strings; "-" is the existing
                // "unset/transparent" sentinel.
                let value = fg.unwrap_or_else(|| "-".to_string());
                match name {
                    "Background" => colors.ui.background_color = value,
                    "Border" => colors.ui.border_color = value,
                    "Command Echo" => colors.ui.command_echo_color = value,
                    "Focused Border" => colors.ui.focused_border_color = value,
                    "System Messages" => colors.ui.system_message_color = value,
                    "Text" => colors.ui.text_color = value,
                    "Text Selection" => colors.ui.selection_bg_color = value,
                    "Textarea Background" => colors.ui.textarea_background = value,
                    other => {
                        tracing::warn!("Unknown UI color entry '{}'", other);
                        return;
                    }
                }
            }
            "PRESETS" => {
                if let Some(preset) = colors.presets.get_mut(name) {
                    preset.fg = fg;
                    preset.bg = bg;
                } else {
                    tracing::warn!("Unknown color preset '{}'", name);
                    return;
                }
            }
            "PROMPT" => {
                let ch = name
                    .strip_prefix("Prompt (")
                    .and_then(|rest| rest.strip_suffix(')'))
                    .unwrap_or(name);
                if let Some(prompt) = colors.prompt_colors.iter_mut().find(|p| p.character == ch) {
                    prompt.color = fg;
                } else {
                    tracing::warn!("Unknown prompt color '{}'", name);
                    return;
                }
            }
            other => {
                tracing::warn!("Unknown UI color category '{}'", other);
                return;
            }
        }
        Self::persist_colors(app_core);
    }

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
        let menu_x = window_pos.x.get();
        let menu_y = if window_pos.y.get() >= menu_height {
            window_pos.y.get() - menu_height
        } else {
            window_pos.y.get().saturating_add(1)
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

    /// Route a mouse event to the open highlight browser (list of rules).
    /// Returns `Ok(None)` when closed; `Ok(Some(..))` when it consumed the
    /// event. Edit/Delete/Add/Close run as simulated key presses.
    fn handle_highlight_browser_mouse(
        &mut self,
        app_core: &mut crate::core::AppCore,
        kind: &crate::data::input::MouseEventKind,
        x: u16,
        y: u16,
        handle_menu_action_fn: &impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

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
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, false, scroll_direction, area)
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    browser.handle_mouse(x, y, false, scroll_direction, area)
                }
                _ => HighlightBrowserMouseAction::None,
            };

            app_core.needs_render = true;

            match action {
                HighlightBrowserMouseAction::Edit => {
                    // Trigger edit via simulated Enter key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Enter,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                HighlightBrowserMouseAction::Delete => {
                    // Trigger delete via simulated Delete key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Delete,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                HighlightBrowserMouseAction::Add => {
                    // Trigger add via simulated 'a' key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('a'),
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                HighlightBrowserMouseAction::Close => {
                    // Trigger close via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                HighlightBrowserMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    /// Route a mouse event to the open keybind browser; Edit/Delete/Add/Close run as simulated key presses. Ok(None) when closed.
    fn handle_keybind_browser_mouse(
        &mut self,
        app_core: &mut crate::core::AppCore,
        kind: &crate::data::input::MouseEventKind,
        x: u16,
        y: u16,
        handle_menu_action_fn: &impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

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
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, false, scroll_direction, area)
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    browser.handle_mouse(x, y, false, scroll_direction, area)
                }
                _ => KeybindBrowserMouseAction::None,
            };

            app_core.needs_render = true;

            match action {
                KeybindBrowserMouseAction::Edit => {
                    // Trigger edit via simulated Enter key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Enter,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                KeybindBrowserMouseAction::Delete => {
                    // Trigger delete via simulated Delete key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Delete,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                KeybindBrowserMouseAction::Add => {
                    // Trigger add via simulated 'a' key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('a'),
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                KeybindBrowserMouseAction::Close => {
                    // Trigger close via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                KeybindBrowserMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    /// Route a mouse event to the open keybind form; Save/Cancel run as simulated key presses. Ok(None) when closed.
    fn handle_keybind_form_mouse(
        &mut self,
        app_core: &mut crate::core::AppCore,
        kind: &crate::data::input::MouseEventKind,
        x: u16,
        y: u16,
        handle_menu_action_fn: &impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

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
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, true, area)
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, true, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, false, area)
                }
                _ => KeybindFormMouseAction::None,
            };

            app_core.needs_render = true;

            match action {
                KeybindFormMouseAction::Save => {
                    // Trigger save via simulated Ctrl+S key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('s'),
                        KeyModifiers::CTRL,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                KeybindFormMouseAction::Delete => {
                    // Trigger delete via simulated Ctrl+D key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('d'),
                        KeyModifiers::CTRL,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                KeybindFormMouseAction::Cancel => {
                    // Trigger cancel via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                KeybindFormMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    /// Route a mouse event to the open color palette browser; actions run as simulated key presses. Ok(None) when closed.
    fn handle_color_palette_browser_mouse(
        &mut self,
        app_core: &mut crate::core::AppCore,
        kind: &crate::data::input::MouseEventKind,
        x: u16,
        y: u16,
        handle_menu_action_fn: &impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

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
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    browser.handle_mouse(x, y, false, scroll_direction, area)
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    browser.handle_mouse(x, y, false, scroll_direction, area)
                }
                _ => ColorPaletteBrowserMouseAction::None,
            };

            app_core.needs_render = true;

            match action {
                ColorPaletteBrowserMouseAction::Edit => {
                    // Trigger edit via simulated Enter key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Enter,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                ColorPaletteBrowserMouseAction::Delete => {
                    // Trigger delete via simulated Delete key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Delete,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                ColorPaletteBrowserMouseAction::Add => {
                    // Trigger add via simulated 'a' key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('a'),
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                ColorPaletteBrowserMouseAction::ToggleFavorite => {
                    // Favorite already toggled in handle_mouse, just trigger save via 'f' key
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('f'),
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                ColorPaletteBrowserMouseAction::Close => {
                    // Trigger close via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                ColorPaletteBrowserMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    /// Route a mouse event to the open color form; Save/Cancel run as simulated key presses. Ok(None) when closed.
    fn handle_color_form_mouse(
        &mut self,
        app_core: &mut crate::core::AppCore,
        kind: &crate::data::input::MouseEventKind,
        x: u16,
        y: u16,
        handle_menu_action_fn: &impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

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
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, true, area)
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, true, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, false, area)
                }
                _ => ColorFormMouseAction::None,
            };

            app_core.needs_render = true;

            match action {
                ColorFormMouseAction::Save => {
                    // Trigger save via simulated Ctrl+S key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('s'),
                        KeyModifiers::CTRL,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                ColorFormMouseAction::Cancel => {
                    // Trigger cancel via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                ColorFormMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    /// Route a mouse event to the open settings editor; actions run as simulated key presses. Ok(None) when closed.
    fn handle_settings_editor_mouse(
        &mut self,
        app_core: &mut crate::core::AppCore,
        kind: &crate::data::input::MouseEventKind,
        x: u16,
        y: u16,
        handle_menu_action_fn: &impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

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
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    editor.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    editor.handle_mouse(x, y, true, scroll_direction, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    editor.handle_mouse(x, y, false, scroll_direction, area)
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    editor.handle_mouse(x, y, false, scroll_direction, area)
                }
                _ => SettingsEditorMouseAction::None,
            };

            app_core.needs_render = true;

            match action {
                SettingsEditorMouseAction::EditValue => {
                    // Trigger edit via simulated Enter key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Enter,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                SettingsEditorMouseAction::ToggleScope => {
                    // Trigger scope toggle via simulated 'g' key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('g'),
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                SettingsEditorMouseAction::Close => {
                    // Trigger close via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                SettingsEditorMouseAction::SelectRow | SettingsEditorMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    /// Route a mouse event to the open highlight form. Returns `Ok(None)`
    /// when the form is closed so the caller falls through; `Ok(Some(..))`
    /// when the form consumed the event (Save/Cancel run as simulated key
    /// presses through `handle_menu_action_fn`).
    fn handle_highlight_form_mouse(
        &mut self,
        app_core: &mut crate::core::AppCore,
        kind: &crate::data::input::MouseEventKind,
        x: u16,
        y: u16,
        handle_menu_action_fn: &impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<(bool, Option<String>)>> {
        use crate::data::input::MouseEventKind;

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
                MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, true, area)
                }
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, true, area)
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                    form.handle_mouse(x, y, false, area)
                }
                _ => HighlightFormMouseAction::None,
            };

            app_core.needs_render = true;

            match action {
                HighlightFormMouseAction::Save => {
                    // Trigger save via simulated Ctrl+S key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Char('s'),
                        KeyModifiers::CTRL,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                HighlightFormMouseAction::Cancel => {
                    // Trigger cancel via simulated Esc key press
                    use crate::data::input::KeyCode;
                    use crate::data::input::KeyModifiers;
                    let _ = self.handle_key_event(
                        KeyCode::Esc,
                        KeyModifiers::NONE,
                        app_core,
                        handle_menu_action_fn,
                    );
                    return Ok(Some((true, None)));
                }
                HighlightFormMouseAction::None => {
                    return Ok(Some((true, None)));
                }
            }
        }

        Ok(None)
    }

    /// Scroll the topmost window under the cursor. `delta` is positive to
    /// scroll up (toward older lines), negative to scroll down.
    fn handle_mouse_scroll(
        &mut self,
        app_core: &mut crate::core::AppCore,
        x: u16,
        y: u16,
        delta: i32,
    ) -> Result<(bool, Option<String>)> {
        // Find topmost window at mouse position (ephemeral windows have higher z-order)
        let target_window = find_topmost_window_at(&app_core.ui_state, x, y);
        self.scroll_window(&target_window, delta);
        app_core.needs_render = true;
        Ok((true, None))
    }

    /// Right-click handling: performance-overlay metrics menu, then window
    /// title-bar context menus. Returns `(false, None)` when the click hits
    /// nothing actionable so the caller can fall through.
    fn handle_mouse_down_right(
        &mut self,
        app_core: &mut crate::core::AppCore,
        x: u16,
        y: u16,
    ) -> Result<(bool, Option<String>)> {
        use crate::data::ui_state::InputMode;

        // Right-click on performance overlay: show metrics toggle menu
        if let Some(window) = app_core.ui_state.windows.get("performance_overlay") {
            let pos = &window.position;
            if x >= pos.x.get()
                && x < pos.x.get() + pos.width.get()
                && y >= pos.y.get()
                && y < pos.y.get() + pos.height.get()
            {
                // Build performance metrics context menu
                let items = Self::build_perf_metrics_context_menu(&app_core.config.ui);
                app_core.ui_state.popup_menu =
                    Some(crate::data::ui_state::PopupMenu::new(items, (x, y + 1)));
                app_core.ui_state.input_mode = InputMode::Menu;
                app_core.needs_render = true;
                return Ok((true, None));
            }
        }

        // Right-click: show context menu for window title bars
        for (name, window) in &app_core.ui_state.windows {
            let pos = &window.position;
            // Check if click is on the title bar (top row of window)
            if y == pos.y.get() && x >= pos.x.get() && x < pos.x.get() + pos.width.get() {
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
                        Some(crate::data::ui_state::PopupMenu::new(items, (x, y + 1)));
                    app_core.ui_state.input_mode = InputMode::Menu;
                    app_core.needs_render = true;
                    return Ok((true, None));
                }
            }
        }

        Ok((false, None))
    }

    /// Left-button release: finalize link drag-and-drop, commit window
    /// drag/resize moves, and close text selections. Returns the command
    /// (if any) produced by a completed link drop.
    fn handle_mouse_up_left(
        &mut self,
        app_core: &mut crate::core::AppCore,
        x: u16,
        y: u16,
    ) -> Result<(bool, Option<String>)> {
        let mut command_to_send: Option<String> = None;

        if let Some(link_drag) = app_core.ui_state.link_drag_state.take() {
            let dx = (x as i16 - link_drag.start_pos.0 as i16).abs();
            let dy = (y as i16 - link_drag.start_pos.1 as i16).abs();

            if dx > 2 || dy > 2 {
                let mut drop_target_hand: Option<String> = None;
                let mut drop_target_id: Option<String> = None;

                for (name, window) in &app_core.ui_state.windows {
                    let pos = &window.position;
                    if x >= pos.x.get()
                        && x < pos.x.get() + pos.width.get()
                        && y >= pos.y.get()
                        && y < pos.y.get() + pos.height.get()
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
                            x: pos.x.get(),
                            y: pos.y.get(),
                            width: pos.width.get(),
                            height: pos.height.get(),
                        };

                        // Inventory window: check link first, fallback to wear
                        if matches!(window.content, crate::data::WindowContent::Inventory(_)) {
                            if let Some(target_link) =
                                self.link_at_position(name, x, y, window_rect)
                            {
                                drop_target_id = Some(target_link.exist_id);
                            } else {
                                drop_target_hand = Some("wear".to_string());
                            }
                            break;
                        }

                        // Container windows: check link first, fallback to container
                        if let crate::data::WindowContent::Container {
                            ref container_title,
                        } = window.content
                        {
                            // First: try to find a link at the drop position (nested container)
                            if let Some(target_link) =
                                self.link_at_position(name, x, y, window_rect)
                            {
                                drop_target_id = Some(target_link.exist_id);
                            } else {
                                // Fallback: the window's container as a
                                // game-command target (command_target is
                                // stow-correct; plain id would be "#stow").
                                if let Some(container_data) =
                                    app_core.game_state.objects.find_container(container_title)
                                {
                                    drop_target_id = Some(container_data.command_target());
                                }
                            }
                            break; // Container window handled
                        }

                        // Items window: check link first, fallback to drop
                        if matches!(window.content, crate::data::WindowContent::Items) {
                            if let Some(target_link) =
                                self.link_at_position(name, x, y, window_rect)
                            {
                                drop_target_id = Some(target_link.exist_id);
                            }
                            // No else - if no link clicked, fall through to "drop" at line ~1782
                            break; // Items window handled
                        }

                        // Otherwise check if we dropped on a link (non-container windows)
                        if let Some(target_link) = self.link_at_position(name, x, y, window_rect) {
                            drop_target_id = Some(target_link.exist_id);
                            break;
                        }
                    }
                }

                let command = if let Some(hand_type) = drop_target_hand {
                    format!("_drag #{} {}\n", link_drag.link_data.exist_id, hand_type)
                } else if let Some(target_id) = drop_target_id {
                    format!("_drag #{} #{}\n", link_drag.link_data.exist_id, target_id)
                } else {
                    format!("_drag #{} drop\n", link_drag.link_data.exist_id)
                };
                command_to_send = Some(command);
            }
        } else if let Some(pending_click) = app_core.ui_state.pending_link_click.take() {
            let dx = (x as i16 - pending_click.click_pos.0 as i16).abs();
            let dy = (y as i16 - pending_click.click_pos.1 as i16).abs();

            if dx <= 2 && dy <= 2 {
                // Web links open in the default browser, nothing goes upstream
                if pending_click.link_data.exist_id == crate::data::URL_LINK_SENTINEL {
                    if crate::data::is_web_url(&pending_click.link_data.noun) {
                        if let Err(err) = crate::platform::open_url(&pending_click.link_data.noun) {
                            app_core.add_system_message(&format!(
                                "Cannot open {}: {}",
                                pending_click.link_data.noun, err
                            ));
                        }
                    }
                // Handle <d> tags differently (direct commands vs context menus)
                } else if pending_click.link_data.exist_id == "_direct_" {
                    // <d> tag: Send text/noun as direct command
                    let command = if !pending_click.link_data.noun.is_empty() {
                        format!("{}\n", pending_click.link_data.noun)
                    // Use cmd attribute
                    } else {
                        format!("{}\n", pending_click.link_data.text)
                        // Use text content
                    };
                    tracing::info!("Executing <d> direct command: {}", command.trim());
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
                tracing::debug!("Link click cancelled - dragged {} pixels", dx.max(dy));
            }
        }

        // Sync UI state positions back to layout WindowDefs after mouse resize/move
        let mut window_layout_changed = false;
        if let Some(drag_state) = &app_core.ui_state.mouse_drag {
            if let Some(window) = app_core.ui_state.get_window(&drag_state.window_name) {
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
                    tracing::info!(
                        "Synced mouse resize/move for '{}' to layout: pos=({},{}) size={}x{}",
                        drag_state.window_name,
                        base.col.get(),
                        base.row.get(),
                        base.cols.get(),
                        base.rows.get()
                    );
                    window_layout_changed = true;
                }

                // Save ephemeral container window positions to widget_state.toml
                if app_core
                    .ui_state
                    .ephemeral_windows
                    .contains(&drag_state.window_name)
                {
                    use crate::config::{Config, DialogPosition};
                    let pos = DialogPosition {
                        x: window.position.x.get(),
                        y: window.position.y.get(),
                        width: Some(window.position.width.get()),
                        height: Some(window.position.height.get()),
                    };
                    app_core
                        .saved_dialog_positions
                        .containers
                        .insert(drag_state.window_name.clone(), pos);
                    // Save to disk asynchronously (best-effort)
                    let character = app_core.config.character.clone();
                    let positions = app_core.saved_dialog_positions.clone();
                    std::thread::spawn(move || {
                        if let Err(e) =
                            Config::save_dialog_positions(character.as_deref(), &positions)
                        {
                            tracing::warn!("Failed to save container positions: {}", e);
                        }
                    });
                    tracing::debug!(
                        "Saved ephemeral container position for '{}'",
                        drag_state.window_name
                    );
                }
            }
        }

        if window_layout_changed {
            app_core.schedule_layout_autosave();
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
                    window_name,
                    start.line,
                    start.col,
                    end.line,
                    end.col,
                ) {
                    // Copy to clipboard
                    match crate::clipboard::copy(&text) {
                        Ok(()) => {
                            tracing::info!(
                                "Copied {} chars to clipboard from '{}'",
                                text.len(),
                                window_name
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Failed to copy to clipboard: {}", e);
                        }
                    }
                }
            }
            // Clear selection and unfreeze text window
            if auto_copy {
                // Unfreeze the text window before clearing selection
                let window_to_unfreeze = selection.window_name.clone();
                if let Some(text_window) = self
                    .widget_manager
                    .text_windows
                    .get_mut(&window_to_unfreeze)
                {
                    text_window.unfreeze_and_apply_pending();
                } else if let Some(tabbed_window) = self
                    .widget_manager
                    .tabbed_text_windows
                    .get_mut(&window_to_unfreeze)
                {
                    tabbed_window.unfreeze_and_apply_pending();
                }
                app_core.ui_state.selection_state = None;
            }
            app_core.needs_render = true;
        }

        Ok((true, command_to_send))
    }

    /// Left-button drag: move an in-progress link drag, apply a window
    /// move/resize gesture, or extend a text selection. `window_names` is
    /// the caller's sorted window-name slice, used to index selections.
    fn handle_mouse_drag_left(
        &mut self,
        app_core: &mut crate::core::AppCore,
        x: u16,
        y: u16,
        window_names: &[String],
    ) -> Result<(bool, Option<String>)> {
        if let Some(ref mut link_drag) = app_core.ui_state.link_drag_state {
            link_drag.current_pos = (x, y);
            app_core.needs_render = true;
        } else if let Some(drag_state) = app_core.ui_state.mouse_drag.clone() {
            let dx = x as i32 - drag_state.start_pos.0 as i32;
            let dy = y as i32 - drag_state.start_pos.1 as i32;

            // Get terminal size for clamping windows within bounds
            let (term_width, term_height) = self.size();

            let (min_width_constraint, min_height_constraint) =
                app_core.window_min_size(&drag_state.window_name);

            if let Some(window) = app_core.ui_state.get_window_mut(&drag_state.window_name) {
                // Geometry lives in the pure, unit-tested
                // apply_window_drag (data/window.rs). The original
                // window position at drag-start is `original_window_pos`
                // (x, y, width, height).
                let (ox, oy, ow, oh) = drag_state.original_window_pos;
                window.position = crate::data::window::apply_window_drag(
                    drag_state.operation,
                    crate::data::WindowPosition {
                        x: crate::data::geometry::Col::new(ox),
                        y: crate::data::geometry::Row::new(oy),
                        width: crate::data::geometry::Width::new(ow),
                        height: crate::data::geometry::Height::new(oh),
                    },
                    dx,
                    dy,
                    min_width_constraint,
                    min_height_constraint,
                    term_width,
                    term_height,
                );
                app_core.needs_render = true;
            }
        } else if app_core.ui_state.pending_link_click.is_some() {
            app_core.ui_state.pending_link_click = None;
        } else if let Some(_drag_start) = app_core.ui_state.selection_drag_start {
            // Update text selection on drag
            if let Some(ref mut selection) = app_core.ui_state.selection_state {
                // Find which window we're dragging in
                for (name, window) in &app_core.ui_state.windows {
                    let pos = &window.position;
                    if x >= pos.x.get()
                        && x < pos.x.get() + pos.width.get()
                        && y >= pos.y.get()
                        && y < pos.y.get() + pos.height.get()
                    {
                        let window_rect = ratatui::layout::Rect {
                            x: pos.x.get(),
                            y: pos.y.get(),
                            width: pos.width.get(),
                            height: pos.height.get(),
                        };
                        if let Some((line, col)) =
                            self.mouse_to_text_coords(name, x, y, window_rect)
                        {
                            let window_index = window_names.binary_search(name).unwrap_or(0);
                            selection.update_end(window_index, line, col);
                            app_core.needs_render = true;
                        }
                        break;
                    }
                }
            }
        }
        Ok((true, None))
    }

    /// Left-button press: the main click hit-test. Handles menu clicks,
    /// link activation/drag-start, window drag/resize initiation, text
    /// selection start, and focus changes. `window_names` is the caller's
    /// sorted window-name slice; `handle_menu_action_fn` runs menu commands.
    fn handle_mouse_down_left(
        &mut self,
        app_core: &mut crate::core::AppCore,
        x: u16,
        y: u16,
        modifiers: &crate::data::input::KeyModifiers,
        window_names: &[String],
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<(bool, Option<String>)> {
        use crate::data::ui_state::InputMode;
        use crate::data::{
            window::WidgetType, DragOperation, LinkDragState, MouseDragState, PendingLinkClick,
        };
        use ratatui::layout::Rect;

        // If in menu mode, handle menu clicks first
        if app_core.ui_state.input_mode == InputMode::Menu {
            let mut clicked_item = None;
            let (screen_width, screen_height) = self.size();

            // Helper to calculate menu area with screen bounds
            let calc_menu_area = |menu: &crate::data::ui_state::PopupMenu| -> (u16, u16, u16, u16) {
                let pos = menu.get_position();
                let menu_height = menu.get_items().len() as u16 + 2; // +2 for borders
                let menu_width = menu
                    .get_items()
                    .iter()
                    .map(|item| item.text.len())
                    .max()
                    .unwrap_or(10) as u16
                    + 4; // +4 for borders and padding
                let max_x = screen_width.saturating_sub(menu_width);
                let max_y = screen_height.saturating_sub(menu_height);
                let menu_x = pos.0.min(max_x);
                let menu_y = pos.1.min(max_y);
                (menu_x, menu_y, menu_width, menu_height)
            };

            // Check menus from DEEPEST to SHALLOWEST (deep_submenu first, popup_menu last)
            // This ensures clicks on submenus are handled before checking parent menus

            // Level 4: deep_submenu (deepest)
            if clicked_item.is_none() {
                if let Some(ref menu) = app_core.ui_state.deep_submenu {
                    let menu_area = calc_menu_area(menu);
                    if let Some(index) = menu.check_click(x, y, menu_area) {
                        clicked_item = menu.get_items().get(index).cloned();
                        tracing::debug!("Click hit deep_submenu item at index {}", index);
                    }
                }
            }

            // Level 3: nested_submenu
            if clicked_item.is_none() {
                if let Some(ref menu) = app_core.ui_state.nested_submenu {
                    let menu_area = calc_menu_area(menu);
                    if let Some(index) = menu.check_click(x, y, menu_area) {
                        clicked_item = menu.get_items().get(index).cloned();
                        tracing::debug!("Click hit nested_submenu item at index {}", index);
                    }
                }
            }

            // Level 2: submenu
            if clicked_item.is_none() {
                if let Some(ref menu) = app_core.ui_state.submenu {
                    let menu_area = calc_menu_area(menu);
                    if let Some(index) = menu.check_click(x, y, menu_area) {
                        clicked_item = menu.get_items().get(index).cloned();
                        tracing::debug!("Click hit submenu item at index {}", index);
                    }
                }
            }

            // Level 1: popup_menu (shallowest)
            if clicked_item.is_none() {
                if let Some(ref menu) = app_core.ui_state.popup_menu {
                    let menu_area = calc_menu_area(menu);
                    if let Some(index) = menu.check_click(x, y, menu_area) {
                        clicked_item = menu.get_items().get(index).cloned();
                        tracing::debug!("Click hit popup_menu item at index {}", index);
                    }
                }
            }

            if let Some(item) = clicked_item {
                let command = item.command.clone();
                tracing::info!("Menu item clicked: {} (command: {})", item.text, command);

                // Dispatch through the same path as keyboard Enter so
                // mouse and keyboard menus can never diverge.
                let result = self.handle_menu_command(command, app_core, &handle_menu_action_fn)?;
                return Ok((true, result));
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
        // Unfreeze any frozen text window before clearing selection
        if let Some(ref selection) = app_core.ui_state.selection_state {
            if let Some(text_window) = self
                .widget_manager
                .text_windows
                .get_mut(&selection.window_name)
            {
                text_window.unfreeze_and_apply_pending();
            } else if let Some(tabbed_window) = self
                .widget_manager
                .tabbed_text_windows
                .get_mut(&selection.window_name)
            {
                tabbed_window.unfreeze_and_apply_pending();
            }
        }
        app_core.ui_state.selection_state = None;

        let topmost_window = find_topmost_window_at(&app_core.ui_state, x, y);
        let (is_quickbar, window_pos) = app_core
            .ui_state
            .get_window(&topmost_window)
            .map(|window| {
                (
                    window.widget_type == WidgetType::Quickbar,
                    Some(window.position.clone()),
                )
            })
            .unwrap_or((false, None));

        if is_quickbar {
            if let Some(quickbar_widget) = self
                .widget_manager
                .quickbar_widgets
                .get_mut(&topmost_window)
            {
                let window_pos = window_pos.unwrap_or(crate::data::WindowPosition {
                    x: crate::data::geometry::Col::new(0),
                    y: crate::data::geometry::Row::new(0),
                    width: crate::data::geometry::Width::new(0),
                    height: crate::data::geometry::Height::new(0),
                });
                let rect = Rect {
                    x: window_pos.x.get(),
                    y: window_pos.y.get(),
                    width: window_pos.width.get(),
                    height: window_pos.height.get(),
                };
                if let Some(action) = quickbar_widget.handle_click(x, y, rect) {
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
                        crate::frontend::tui::quickbar::QuickbarAction::MenuRequest {
                            exist,
                            noun,
                        } => {
                            let command = app_core.request_menu(exist, noun, (x, y));
                            return Ok((true, Some(command)));
                        }
                    }
                }
            }
        }

        let is_hotkeybar = app_core
            .ui_state
            .get_window(&topmost_window)
            .map(|window| window.widget_type == WidgetType::Hotkeybar)
            .unwrap_or(false);
        if is_hotkeybar {
            if let Some(bar_widget) = self
                .widget_manager
                .hotkey_bar_widgets
                .get_mut(&topmost_window)
            {
                let window_pos = app_core
                    .ui_state
                    .get_window(&topmost_window)
                    .map(|w| w.position.clone())
                    .unwrap_or(crate::data::WindowPosition {
                        x: crate::data::geometry::Col::new(0),
                        y: crate::data::geometry::Row::new(0),
                        width: crate::data::geometry::Width::new(0),
                        height: crate::data::geometry::Height::new(0),
                    });
                let rect = Rect {
                    x: window_pos.x.get(),
                    y: window_pos.y.get(),
                    width: window_pos.width.get(),
                    height: window_pos.height.get(),
                };
                if let Some(command) = bar_widget.handle_click(x, y, rect) {
                    app_core.needs_render = true;
                    return Ok((true, Some(format!("{}\n", command))));
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
            x,
            y,
            topmost_window
        );

        if let Some(window) = app_core.ui_state.get_window(&topmost_window) {
            tracing::debug!(
                "  Window pos: y={}, height={}, click_y={}, is_top_row={}",
                window.position.y.get(),
                window.position.height.get(),
                y,
                y == window.position.y.get()
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
                    x: pos.x.get(),
                    y: pos.y.get(),
                    width: pos.width.get(),
                    height: pos.height.get(),
                };
                if let Some(new_index) = self.handle_tabbed_click(name, rect, x, y) {
                    handled_tab_click = Some((name.clone(), new_index));
                }
            }

            if handled_tab_click.is_none() {
                if let Some(op) = resize_op_at(
                    pos.x.get(),
                    pos.y.get(),
                    pos.width.get(),
                    pos.height.get(),
                    is_window_locked,
                    x,
                    y,
                ) {
                    drag_op = Some(op);
                    found_window = Some(name.clone());
                }
            }
        }

        if let Some((win_name, new_index)) = handled_tab_click {
            // Focus the tabbed window when clicking its tabs
            app_core.ui_state.set_focus(Some(win_name.clone()));

            if let Some(window_state) = app_core.ui_state.get_window_mut(&win_name) {
                if let crate::data::WindowContent::TabbedText(tabbed) = &mut window_state.content {
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
                            x: pos.x.get(),
                            y: pos.y.get(),
                            width: pos.width.get(),
                            height: pos.height.get(),
                        };

                        if let Some(link_data) =
                            self.link_at_position(&window_name, x, y, window_rect)
                        {
                            if has_ctrl {
                                // Ctrl+click always starts link drag
                                app_core.ui_state.link_drag_state = Some(LinkDragState {
                                    link_data,
                                    start_pos: (x, y),
                                    current_pos: (x, y),
                                });
                            } else {
                                // Locked window without Ctrl: open menu
                                app_core.ui_state.pending_link_click = Some(PendingLinkClick {
                                    link_data,
                                    click_pos: (x, y),
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
                        start_pos: (x, y),
                        original_window_pos: (
                            pos.x.get(),
                            pos.y.get(),
                            pos.width.get(),
                            pos.height.get(),
                        ),
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
                    x: pos.x.get(),
                    y: pos.y.get(),
                    width: pos.width.get(),
                    height: pos.height.get(),
                };

                tracing::debug!(
                    "Non-drag click on '{}' at ({}, {}), window_rect: y={}, height={}",
                    window_name,
                    x,
                    y,
                    window_rect.y,
                    window_rect.height
                );

                if let Some(link_data) = self.link_at_position(&window_name, x, y, window_rect) {
                    tracing::debug!("  Found link: {}", link_data.noun);
                    let has_ctrl = modifiers.ctrl;

                    if has_ctrl {
                        app_core.ui_state.link_drag_state = Some(LinkDragState {
                            link_data,
                            start_pos: (x, y),
                            current_pos: (x, y),
                        });
                    } else {
                        app_core.ui_state.pending_link_click = Some(PendingLinkClick {
                            link_data,
                            click_pos: (x, y),
                        });
                    }
                } else {
                    // Start text selection
                    app_core.ui_state.selection_drag_start = Some((x, y));

                    // Convert mouse coords to text coords for selection
                    if let Some((line, col)) =
                        self.mouse_to_text_coords(&window_name, x, y, window_rect)
                    {
                        // Find window index from the stable (sorted) ordering
                        let window_index = window_names.binary_search(&window_name).unwrap_or(0);
                        app_core.ui_state.selection_state =
                            Some(crate::selection::SelectionState::new(
                                window_index,
                                line,
                                col,
                                window_name.clone(),
                            ));

                        // Freeze the text window if it's scrolled back
                        // This prevents new lines from shifting selection indices
                        if let Some(text_window) =
                            self.widget_manager.text_windows.get_mut(&window_name)
                        {
                            if text_window.is_scrolled_back() {
                                text_window.freeze_for_selection();
                            }
                        } else if let Some(tabbed_window) = self
                            .widget_manager
                            .tabbed_text_windows
                            .get_mut(&window_name)
                        {
                            if tabbed_window.is_scrolled_back() {
                                tabbed_window.freeze_for_selection();
                            }
                        }
                    }
                }
            }

            // Apply focus after borrow on windows ends
            if should_focus {
                app_core.ui_state.set_focus(Some(window_name));
            }
        }
        Ok((true, None))
    }

    /// Handle mouse events (extracted from main.rs Phase 4.1)
    /// Returns (handled, optional_command)
    pub fn handle_mouse_event(
        &mut self,
        mouse_event: &crate::data::input::MouseEvent,
        app_core: &mut crate::core::AppCore,
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<(bool, Option<String>)> {
        use crate::data::input::MouseEventKind;
        use crate::data::ui_state::InputMode;
        use crate::data::{DialogDragOperation, DialogDragState};
        use crate::frontend::tui::dialog;
        use ratatui::layout::Rect;

        let kind = &mouse_event.kind;
        let x = &mouse_event.column;
        let y = &mouse_event.row;
        let modifiers = &mouse_event.modifiers;

        // Handle injuries popup (any click closes it)
        if app_core.ui_state.injuries_popup.is_some() {
            if let MouseEventKind::Down(_) = kind {
                tracing::debug!("Closing injuries popup via mouse click");
                app_core.ui_state.injuries_popup = None;
                return Ok((true, None));
            }
        }

        // Handle window editor mouse events first (if open)
        if self.window_editor.is_some() {
            if let Some(result) = self.handle_window_editor_mouse(mouse_event, app_core)? {
                return Ok(result);
            }
        }

        // Handle highlight form mouse events
        if self.highlight_form.is_some() {
            if let Some(result) =
                self.handle_highlight_form_mouse(app_core, kind, *x, *y, &handle_menu_action_fn)?
            {
                return Ok(result);
            }
        }

        // Handle highlight browser mouse events
        if self.highlight_browser.is_some() {
            if let Some(result) =
                self.handle_highlight_browser_mouse(app_core, kind, *x, *y, &handle_menu_action_fn)?
            {
                return Ok(result);
            }
        }

        // Handle keybind browser mouse events
        if self.keybind_browser.is_some() {
            if let Some(result) =
                self.handle_keybind_browser_mouse(app_core, kind, *x, *y, &handle_menu_action_fn)?
            {
                return Ok(result);
            }
        }

        // Handle keybind form mouse events
        if self.keybind_form.is_some() {
            if let Some(result) =
                self.handle_keybind_form_mouse(app_core, kind, *x, *y, &handle_menu_action_fn)?
            {
                return Ok(result);
            }
        }

        // Handle color palette browser mouse events
        if self.color_palette_browser.is_some() {
            if let Some(result) = self.handle_color_palette_browser_mouse(
                app_core,
                kind,
                *x,
                *y,
                &handle_menu_action_fn,
            )? {
                return Ok(result);
            }
        }

        // Handle color form mouse events
        if self.color_form.is_some() {
            if let Some(result) =
                self.handle_color_form_mouse(app_core, kind, *x, *y, &handle_menu_action_fn)?
            {
                return Ok(result);
            }
        }

        // Handle settings editor mouse events
        if self.settings_editor.is_some() {
            if let Some(result) =
                self.handle_settings_editor_mouse(app_core, kind, *x, *y, &handle_menu_action_fn)?
            {
                return Ok(result);
            }
        }

        // Spell color popups are modal: title-bar drag plus wheel scroll in
        // the browser; every other mouse event is consumed while open so
        // clicks don't fall through to the windows underneath.
        if self.spell_color_form.is_some() {
            if let Some(result) = self.handle_spell_color_form_mouse(mouse_event, app_core)? {
                return Ok(result);
            }
        }

        if self.spell_color_browser.is_some() {
            if let Some(result) = self.handle_spell_color_browser_mouse(mouse_event, app_core)? {
                return Ok(result);
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
                MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                    if let Some(ref drag_state) = app_core.ui_state.dialog_drag {
                        if let Some(ref mut dialog) = app_core.ui_state.active_dialog {
                            let dx = *x as i32 - drag_state.start_pos.0 as i32;
                            let dy = *y as i32 - drag_state.start_pos.1 as i32;

                            let min_width: u16 = 10;
                            let min_height: u16 = 4;

                            match drag_state.operation {
                                DialogDragOperation::Move => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx)
                                        .max(0)
                                        as u16;
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy)
                                        .max(0)
                                        as u16;

                                    // Get current dialog size to clamp position
                                    let dialog_size =
                                        dialog.size.unwrap_or(drag_state.original_dialog_size);
                                    let max_x = term_width.saturating_sub(dialog_size.0);
                                    let max_y = term_height.saturating_sub(dialog_size.1);

                                    dialog.position = Some((new_x.min(max_x), new_y.min(max_y)));
                                }
                                DialogDragOperation::ResizeRight => {
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + dx)
                                        .max(min_width as i32)
                                        as u16;
                                    let max_width =
                                        term_width.saturating_sub(drag_state.original_dialog_pos.0);
                                    dialog.size = Some((
                                        new_width.min(max_width),
                                        drag_state.original_dialog_size.1,
                                    ));
                                }
                                DialogDragOperation::ResizeBottom => {
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + dy)
                                        .max(min_height as i32)
                                        as u16;
                                    let max_height = term_height
                                        .saturating_sub(drag_state.original_dialog_pos.1);
                                    dialog.size = Some((
                                        drag_state.original_dialog_size.0,
                                        new_height.min(max_height),
                                    ));
                                }
                                DialogDragOperation::ResizeBottomRight => {
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + dx)
                                        .max(min_width as i32)
                                        as u16;
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + dy)
                                        .max(min_height as i32)
                                        as u16;
                                    let max_width =
                                        term_width.saturating_sub(drag_state.original_dialog_pos.0);
                                    let max_height = term_height
                                        .saturating_sub(drag_state.original_dialog_pos.1);
                                    dialog.size = Some((
                                        new_width.min(max_width),
                                        new_height.min(max_height),
                                    ));
                                }
                                DialogDragOperation::ResizeLeft => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx)
                                        .max(0)
                                        as u16;
                                    let width_delta =
                                        drag_state.original_dialog_pos.0 as i32 - new_x as i32;
                                    let new_width = (drag_state.original_dialog_size.0 as i32
                                        + width_delta)
                                        .max(min_width as i32)
                                        as u16;
                                    if new_width >= min_width {
                                        dialog.position =
                                            Some((new_x, drag_state.original_dialog_pos.1));
                                        dialog.size =
                                            Some((new_width, drag_state.original_dialog_size.1));
                                    }
                                }
                                DialogDragOperation::ResizeTop => {
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy)
                                        .max(0)
                                        as u16;
                                    let height_delta =
                                        drag_state.original_dialog_pos.1 as i32 - new_y as i32;
                                    let new_height = (drag_state.original_dialog_size.1 as i32
                                        + height_delta)
                                        .max(min_height as i32)
                                        as u16;
                                    if new_height >= min_height {
                                        dialog.position =
                                            Some((drag_state.original_dialog_pos.0, new_y));
                                        dialog.size =
                                            Some((drag_state.original_dialog_size.0, new_height));
                                    }
                                }
                                DialogDragOperation::ResizeTopLeft => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx)
                                        .max(0)
                                        as u16;
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy)
                                        .max(0)
                                        as u16;
                                    let width_delta =
                                        drag_state.original_dialog_pos.0 as i32 - new_x as i32;
                                    let height_delta =
                                        drag_state.original_dialog_pos.1 as i32 - new_y as i32;
                                    let new_width = (drag_state.original_dialog_size.0 as i32
                                        + width_delta)
                                        .max(min_width as i32)
                                        as u16;
                                    let new_height = (drag_state.original_dialog_size.1 as i32
                                        + height_delta)
                                        .max(min_height as i32)
                                        as u16;
                                    if new_width >= min_width && new_height >= min_height {
                                        dialog.position = Some((new_x, new_y));
                                        dialog.size = Some((new_width, new_height));
                                    }
                                }
                                DialogDragOperation::ResizeTopRight => {
                                    let new_y = (drag_state.original_dialog_pos.1 as i32 + dy)
                                        .max(0)
                                        as u16;
                                    let new_width = (drag_state.original_dialog_size.0 as i32 + dx)
                                        .max(min_width as i32)
                                        as u16;
                                    let height_delta =
                                        drag_state.original_dialog_pos.1 as i32 - new_y as i32;
                                    let new_height = (drag_state.original_dialog_size.1 as i32
                                        + height_delta)
                                        .max(min_height as i32)
                                        as u16;
                                    let max_width =
                                        term_width.saturating_sub(drag_state.original_dialog_pos.0);
                                    if new_height >= min_height {
                                        dialog.position =
                                            Some((drag_state.original_dialog_pos.0, new_y));
                                        dialog.size = Some((new_width.min(max_width), new_height));
                                    }
                                }
                                DialogDragOperation::ResizeBottomLeft => {
                                    let new_x = (drag_state.original_dialog_pos.0 as i32 + dx)
                                        .max(0)
                                        as u16;
                                    let new_height = (drag_state.original_dialog_size.1 as i32 + dy)
                                        .max(min_height as i32)
                                        as u16;
                                    let width_delta =
                                        drag_state.original_dialog_pos.0 as i32 - new_x as i32;
                                    let new_width = (drag_state.original_dialog_size.0 as i32
                                        + width_delta)
                                        .max(min_width as i32)
                                        as u16;
                                    let max_height = term_height
                                        .saturating_sub(drag_state.original_dialog_pos.1);
                                    if new_width >= min_width {
                                        dialog.position =
                                            Some((new_x, drag_state.original_dialog_pos.1));
                                        dialog.size = Some((new_width, new_height.min(max_height)));
                                    }
                                }
                            }
                            app_core.needs_render = true;
                        }
                    }
                    return Ok((true, None));
                }
                MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
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
                                    app_core
                                        .saved_dialog_positions
                                        .dialogs
                                        .insert(dialog.id.clone(), pos);
                                    // Save to disk asynchronously (best-effort)
                                    let character = app_core.config.character.clone();
                                    let positions = app_core.saved_dialog_positions.clone();
                                    std::thread::spawn(move || {
                                        if let Err(e) = Config::save_dialog_positions(
                                            character.as_deref(),
                                            &positions,
                                        ) {
                                            tracing::warn!(
                                                "Failed to save dialog positions: {}",
                                                e
                                            );
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

                if let MouseEventKind::Down(crate::data::input::MouseButton::Left) = kind {
                    let layout = dialog::compute_dialog_layout(screen_area, dialog_state);

                    // Check resize handles first
                    if let Some(resize_op) = dialog::hit_test_resize_handle(&layout, *x, *y) {
                        let current_pos = dialog_state
                            .position
                            .unwrap_or((layout.area.x, layout.area.y));
                        let current_size = dialog_state
                            .size
                            .unwrap_or((layout.area.width, layout.area.height));
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
                        let current_pos = dialog_state
                            .position
                            .unwrap_or((layout.area.x, layout.area.y));
                        let current_size = dialog_state
                            .size
                            .unwrap_or((layout.area.width, layout.area.height));
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
                        let (cmd, should_close) = dialog_state.activate_button(index);
                        command_to_send = cmd;
                        close_dialog = should_close;
                    } else if let Some(index) = dialog::hit_test_dropdown(&layout, *x, *y) {
                        // Click-to-cycle: advance the dropdown and fire its
                        // command (resolved through %id% substitution).
                        Self::set_dialog_focus(dialog_state, None);
                        command_to_send = dialog_state.cycle_dropdown(index);
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

        // Stable window index = position in the sorted name list. A binary
        // search at the two use sites replaces the HashMap previously built
        // on every mouse event.
        let mut window_names: Vec<String> = app_core.ui_state.windows.keys().cloned().collect();
        window_names.sort();

        match kind {
            MouseEventKind::ScrollUp => {
                return self.handle_mouse_scroll(app_core, *x, *y, 10);
            }
            MouseEventKind::ScrollDown => {
                return self.handle_mouse_scroll(app_core, *x, *y, -10);
            }
            MouseEventKind::Down(crate::data::input::MouseButton::Left) => {
                return self.handle_mouse_down_left(
                    app_core,
                    *x,
                    *y,
                    modifiers,
                    &window_names,
                    handle_menu_action_fn,
                );
            }
            MouseEventKind::Drag(crate::data::input::MouseButton::Left) => {
                return self.handle_mouse_drag_left(app_core, *x, *y, &window_names);
            }
            MouseEventKind::Up(crate::data::input::MouseButton::Left) => {
                return self.handle_mouse_up_left(app_core, *x, *y);
            }
            MouseEventKind::Down(crate::data::input::MouseButton::Right) => {
                return self.handle_mouse_down_right(app_core, *x, *y);
            }
            _ => {}
        }

        Ok((false, None))
    }

    /// Handle keyboard events (extracted from main.rs Phase 4.2)
    /// Returns optional command to send to server
    pub fn handle_key_event(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<String>> {
        use crate::data::input::{KeyCode, KeyModifiers};
        use crate::data::ui_state::InputMode;

        tracing::debug!(
            "Key event: code={:?}, modifiers={:?}, input_mode={:?}",
            code,
            modifiers,
            app_core.ui_state.input_mode
        );

        // Handle injuries popup (overlay that closes on Escape or any click outside)
        if app_core.ui_state.injuries_popup.is_some() {
            if code == KeyCode::Esc && modifiers == KeyModifiers::NONE {
                tracing::debug!("Closing injuries popup via Escape");
                app_core.ui_state.injuries_popup = None;
                return Ok(None);
            }
        }

        // Skill trainer panel: ui_state-driven overlay (core opens it when
        // the user sends `goals`), so it claims all keys while open.
        if app_core.ui_state.skill_trainer.open {
            return self.handle_skill_trainer_panel_keys(code, modifiers, app_core);
        }

        // LAYER 1 & 2: Priority windows (browsers, forms, editors) - handle ALL keys
        // These modes get first priority and consume most input
        match app_core.ui_state.input_mode {
            InputMode::Dialog => {
                let result = self.handle_dialog_mode_keys(code, modifiers, app_core)?;
                return Ok(result);
            }
            InputMode::HighlightBrowser => {
                return self.handle_highlight_browser_mode_keys(code, modifiers, app_core);
            }
            InputMode::KeybindBrowser => {
                return self.handle_keybind_browser_mode_keys(code, modifiers, app_core)
            }
            InputMode::HotbarEditor => {
                return self.handle_hotbar_editor_mode_keys(code, modifiers, app_core)
            }
            InputMode::ColorPaletteBrowser => {
                return self.handle_color_palette_browser_mode_keys(code, modifiers, app_core)
            }
            InputMode::UIColorsBrowser => {
                return self.handle_uicolors_browser_mode_keys(code, modifiers, app_core)
            }
            InputMode::SpellColorsBrowser => {
                return self.handle_spell_colors_browser_mode_keys(code, modifiers, app_core)
            }
            InputMode::ThemeBrowser => {
                return self.handle_theme_browser_mode_keys(code, modifiers, app_core)
            }
            InputMode::SettingsEditor => {
                return self.handle_settings_editor_mode_keys(code, modifiers, app_core)
            }
            InputMode::PackEditor => {
                return self.handle_pack_editor_mode_keys(code, modifiers, app_core)
            }
            InputMode::HighlightForm => {
                return self.handle_highlight_form_mode_keys(code, modifiers, app_core)
            }
            InputMode::KeybindForm => {
                return self.handle_keybind_form_mode_keys(code, modifiers, app_core)
            }
            InputMode::ColorForm => {
                return self.handle_color_form_mode_keys(code, modifiers, app_core)
            }
            InputMode::SpellColorForm => {
                return self.handle_spell_color_form_mode_keys(code, modifiers, app_core)
            }
            InputMode::MenuKeybindEditor => {
                return self.handle_menu_keybind_editor_mode_keys(code, modifiers, app_core)
            }
            InputMode::StatusAbbrevEditor => {
                return self.handle_status_abbrev_editor_mode_keys(code, modifiers, app_core)
            }
            InputMode::ThemeEditor => {
                return self.handle_theme_editor_mode_keys(code, modifiers, app_core)
            }
            InputMode::IndicatorTemplateEditor => {
                return self.handle_indicator_template_editor_mode_keys(code, modifiers, app_core)
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
            return self.handle_search_mode_keys(code, modifiers, app_core);
        }

        // Normal mode: user keybinds + CommandInput fallback
        self.handle_normal_mode_keys(code, modifiers, app_core)
    }

    /// Handle Dialog mode keyboard navigation
    fn handle_dialog_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        _modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::data::input::KeyCode;
        use crate::data::ui_state::InputMode;

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
                        if field.move_left() {
                            app_core.needs_render = true;
                        }
                    }
                    KeyCode::Right => {
                        if field.move_right() {
                            app_core.needs_render = true;
                        }
                    }
                    KeyCode::Home => {
                        field.move_home();
                        app_core.needs_render = true;
                    }
                    KeyCode::End => {
                        field.move_end();
                        app_core.needs_render = true;
                    }
                    KeyCode::Backspace => {
                        field.backspace();
                        app_core.needs_render = true;
                    }
                    KeyCode::Delete => {
                        field.delete_forward();
                        app_core.needs_render = true;
                    }
                    KeyCode::Enter => {
                        if let Some(button_id) = field.enter_button.clone() {
                            if let Some(index) = Self::find_dialog_button_index(dialog, &button_id)
                            {
                                dialog.selected = index;
                                let (cmd, should_close) = dialog.activate_button(index);
                                command_to_send = cmd;
                                close_dialog = should_close;
                            }
                        }
                        app_core.needs_render = true;
                    }
                    KeyCode::Char(ch) => {
                        field.insert_char(ch);
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
                            let (cmd, should_close) = dialog.activate_button(dialog.selected);
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

    fn find_dialog_button_index(dialog: &crate::data::DialogState, id: &str) -> Option<usize> {
        dialog.buttons.iter().position(|button| button.id == id)
    }

    fn set_dialog_focus(dialog: &mut crate::data::DialogState, focused: Option<usize>) {
        dialog.focused_field = focused.filter(|idx| *idx < dialog.fields.len());
        for (idx, field) in dialog.fields.iter_mut().enumerate() {
            field.focused = dialog.focused_field == Some(idx);
            field.clamp_cursor();
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

    /// Handle Menu mode keyboard navigation (extracted from main.rs Phase 4.2)
    fn handle_menu_mode_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        _modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
        handle_menu_action_fn: impl Fn(&mut crate::core::AppCore, &mut Self, &str) -> Result<()>,
    ) -> Result<Option<String>> {
        use crate::data::input::KeyCode;
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

    /// Add a window from the add-window menu (`__ADD__<name>`): create it,
    /// mirror it into UI state at the current terminal size, and open its
    /// editor. Template names may auto-rename (spacer, tabbedtext_custom, …),
    /// so the actual def is looked up by name then falls back to the last
    /// window added.
    fn menu_add_window(&mut self, window_name: &str, app_core: &mut crate::core::AppCore) {
        use crate::data::ui_state::InputMode;
        match app_core.layout.add_window(window_name) {
            Ok(_) => {
                let (width, height) = self.size();
                let window_def = app_core
                    .layout
                    .get_window(window_name)
                    .cloned()
                    .or_else(|| app_core.layout.windows.last().cloned());
                if let Some(window_def) = window_def {
                    let actual_name = window_def.name().to_string();
                    app_core.add_new_window(&window_def, width, height);
                    app_core.schedule_layout_autosave();
                    app_core.add_system_message(&format!("Window '{}' added", actual_name));
                    tracing::info!("Added window: {}", actual_name);
                    self.window_editor = Some(
                        crate::frontend::tui::window_editor::WindowEditor::new(window_def),
                    );
                    app_core.ui_state.input_mode = InputMode::WindowEditor;
                } else {
                    app_core.add_system_message(&format!(
                        "Window '{}' added but couldn't retrieve definition",
                        window_name
                    ));
                    app_core.ui_state.input_mode = InputMode::Normal;
                }
            }
            Err(e) => {
                app_core.add_system_message(&format!("Failed to add window: {}", e));
                tracing::error!("Failed to add window '{}': {}", window_name, e);
            }
        }
        app_core.ui_state.close_all_menus();
        app_core.needs_render = true;
    }

    /// Start a blank/custom window editor for a widget type
    /// (`__ADD_CUSTOM__<type>`), naming it `custom_<type>[_n]` uniquely.
    /// No-op if a window editor is already open.
    fn menu_add_custom_window(&mut self, widget_type: &str, app_core: &mut crate::core::AppCore) {
        use crate::data::ui_state::InputMode;
        use crate::frontend::tui::window_editor::WindowEditor;
        if self.window_editor.is_some() {
            tracing::debug!("Window editor already open, ignoring add custom request");
            return;
        }
        let mut editor = WindowEditor::new_window(widget_type.to_string());
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
                "knownwindows" => app_core.build_known_windows_menu(),
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
                app_core.ui_state.nested_submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
            } else if app_core.ui_state.popup_menu.is_some() {
                // Have popup_menu, create submenu
                let parent_pos = app_core
                    .ui_state
                    .popup_menu
                    .as_ref()
                    .map(|m| m.get_position())
                    .unwrap_or((40, 12));
                app_core.ui_state.submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
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
                // Category menu is at nested_submenu (level 3); the template
                // menu opens one level deeper (deep_submenu).
                let pos = app_core.ui_state.child_menu_pos();
                app_core.ui_state.deep_submenu = Some(PopupMenu::new(items, pos));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_INDICATORS" {
            // Indicator submenu under Status (replaces deep_submenu since we're at level 4)
            let templates = crate::core::local_catalog::addable_by_category(
                &app_core.layout,
                app_core.game_type(),
            )
            .get(&crate::config::WidgetCategory::Status)
            .cloned()
            .unwrap_or_default();
            let items = app_core.build_indicator_add_menu(&templates);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let pos = app_core.ui_state.child_menu_pos();
                app_core.ui_state.deep_submenu = Some(PopupMenu::new(items, pos));
            }
            app_core.needs_render = true;
        } else if let Some(category_str) = command.strip_prefix("__SUBMENU_HIDE__") {
            let category = Self::parse_widget_category(category_str, app_core)?;
            let items = app_core.build_hide_window_category_menu(&category);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let pos = app_core.ui_state.child_menu_pos();
                app_core.ui_state.deep_submenu = Some(PopupMenu::new(items, pos));
            }
            app_core.needs_render = true;
        } else if let Some(category_str) = command.strip_prefix("__SUBMENU_EDIT__") {
            let category = Self::parse_widget_category(category_str, app_core)?;
            let items = app_core.build_edit_window_category_menu(&category);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let pos = app_core.ui_state.child_menu_pos();
                app_core.ui_state.deep_submenu = Some(PopupMenu::new(items, pos));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_HIDE_INDICATORS" {
            // Indicator hide submenu (replaces deep_submenu since we're at level 4)
            let indicators = app_core
                .layout
                .windows
                .iter()
                .filter(|w| {
                    w.base().visibility.is_shown() && matches!(w.widget_type(), "indicator")
                })
                .map(|w| w.name().to_string())
                .collect::<Vec<String>>();
            let items = app_core.build_indicator_hide_menu(&indicators);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let pos = app_core.ui_state.child_menu_pos();
                app_core.ui_state.deep_submenu = Some(PopupMenu::new(items, pos));
            }
            app_core.needs_render = true;
        } else if command == "__SUBMENU_EDIT_INDICATORS" {
            // Indicator edit submenu (replaces deep_submenu since we're at level 4)
            let indicators = app_core
                .layout
                .windows
                .iter()
                // Hidden indicators stay editable, matching the edit picker.
                .filter(|w| matches!(w.widget_type(), "indicator"))
                .map(|w| w.name().to_string())
                .collect::<Vec<String>>();
            let items = app_core.build_indicator_edit_menu(&indicators);
            if items.is_empty() {
                app_core.ui_state.deep_submenu = None;
            } else {
                let pos = app_core.ui_state.child_menu_pos();
                app_core.ui_state.deep_submenu = Some(PopupMenu::new(items, pos));
            }
            app_core.needs_render = true;
        } else if command == "__INDICATOR_EDITOR" {
            self.indicator_template_editor = Some(
                crate::frontend::tui::indicator_template_editor::IndicatorTemplateEditor::new(),
            );
            app_core.ui_state.close_all_menus();
            app_core.ui_state.input_mode =
                crate::data::ui_state::InputMode::IndicatorTemplateEditor;
            app_core.needs_render = true;
        } else if let Some(widget_type) = command.strip_prefix("__ADD_CUSTOM__") {
            self.menu_add_custom_window(widget_type, app_core);
        } else if let Some(window_name) = command.strip_prefix("__ADD__") {
            self.menu_add_window(window_name, app_core);
        } else if let Some(window_name) = command.strip_prefix("__HIDE__") {
            match app_core.layout.hide_window(window_name) {
                Ok(_) => {
                    app_core.ui_state.remove_window(window_name);
                    app_core.schedule_layout_autosave();
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
                tracing::debug!(
                    "Window editor already open, ignoring edit request for: {}",
                    window_name
                );
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
            app_core.ui_state.close_all_menus();
            app_core.needs_render = true;
        } else if let Some(window_name) = command.strip_prefix("__CLOSE_WINDOW__") {
            // Handle window close from right-click menu
            app_core.ui_state.close_all_menus();
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
        } else if let Some(name) = command.strip_prefix("__TOGGLE_WINDOW__") {
            // Windows list (keyboard Enter): flip this window's show/hide
            // and close the menu (U3, keyed on window name).
            let name = name.to_string();
            app_core.toggle_known_window(&name);
            app_core.ui_state.close_all_menus();
            app_core.ui_state.input_mode = InputMode::Normal;
            app_core.needs_render = true;
        } else if command == "__PERF_MENU_CLOSE__" {
            // Close the perf metrics menu
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.input_mode = InputMode::Normal;
            app_core.needs_render = true;
        } else if let Some(metric) = command.strip_prefix("__TOGGLE_PERF__") {
            // Performance metric toggle from the right-click menu; ids come
            // from performance::PERF_METRICS.
            if !app_core.config.ui.toggle_perf_metric(metric) {
                tracing::warn!("Unknown perf metric toggle: {metric}");
            }
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
                // Process the dot command (e.g., .menu, .help); UI
                // outcomes are performed here like any menu pick.
                match app_core.send_command(command.to_string()) {
                    Ok(crate::data::CommandOutcome::Ui(action)) => {
                        crate::frontend::tui::menu_actions::handle_ui_action(
                            app_core, self, action,
                        )?;
                    }
                    Ok(crate::data::CommandOutcome::Game(outbound)) => {
                        app_core.finish_game_command_send(&outbound, false);
                    }
                    Err(e) => {
                        tracing::error!("Dot command error: {}", e);
                    }
                    _ => {}
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

        match WidgetCategory::from_name(category_str) {
            Some(category) => Ok(category),
            None => {
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
    fn build_perf_metrics_context_menu(
        ui: &crate::config::UiConfig,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        use crate::performance::{PerfFrontend, PERF_METRICS};
        let check = |on: bool| if on { "✓" } else { " " };
        // Rows derive from the shared metric table (TUI scope), so this
        // menu can never list a metric the widget doesn't render.
        let mut items: Vec<crate::data::ui_state::PopupMenuItem> = PERF_METRICS
            .iter()
            .filter(|metric| metric.in_scope(PerfFrontend::Tui))
            .map(|metric| crate::data::ui_state::PopupMenuItem {
                text: format!(
                    "[{}] {}",
                    check(ui.perf_metric_enabled(metric.id)),
                    metric.label
                ),
                command: format!("__TOGGLE_PERF__{}", metric.id),
                disabled: false,
            })
            .collect();
        items.push(crate::data::ui_state::PopupMenuItem {
            text: format!("[{}] Sparklines", check(ui.perf_sparklines)),
            command: "__TOGGLE_PERF__sparklines".to_string(),
            disabled: false,
        });
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "─────────────".to_string(),
            command: String::new(),
            disabled: true,
        });
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Close".to_string(),
            command: "__PERF_MENU_CLOSE__".to_string(),
            disabled: false,
        });
        items
    }

    /// Handle WindowEditor mode keyboard events (extracted from main.rs Phase 4.2)
    pub(super) fn handle_window_editor_keys(
        &mut self,
        code: crate::data::input::KeyCode,
        modifiers: crate::data::input::KeyModifiers,
        app_core: &mut crate::core::AppCore,
    ) -> Result<Option<String>> {
        use crate::core::input_router;
        use crate::data::input::KeyCode;
        use crate::data::ui_state::InputMode;
        if let Some(ref mut editor) = self.window_editor {
            let key_event = crate::data::input::KeyEvent { code, modifiers };
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
                        || editor.is_on_edit_metrics()
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
                            || editor.is_on_edit_metrics()
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
                                || editor.is_on_edit_metrics()
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
                                tracing::warn!(
                                    "Failed to save window template {}: {}",
                                    window_def.name(),
                                    e
                                );
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
                                // Clear old cached widget before update (handles type changes)
                                let window_name = window_def.name().to_string();
                                app_core.ui_state.widgets_to_reset.push(window_name.clone());

                                *existing = window_def.clone();
                                tracing::info!("Updated window: {}", window_def.name());
                                app_core.update_window_position(&window_def, width, height);

                                // For TabbedText windows, sync tabs and reset widget cache if structure changed
                                if matches!(window_def, crate::config::WindowDef::TabbedText { .. })
                                {
                                    if app_core.sync_tabbed_window_tabs(&window_name) {
                                        app_core.ui_state.needs_widget_reset = true;
                                    }
                                }
                                // Text windows: push streams/compact/timestamps
                                // onto the live content so the change (e.g. the
                                // bounty condense toggle) applies immediately
                                // instead of on window recreation.
                                app_core.apply_text_content_settings(&window_def);
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
                        // If a sub-editor is active, delegate Delete to it (e.g., delete tab in TabEditor)
                        if editor.is_sub_editor_active() {
                            let tf_key = crate::data::input::KeyEvent {
                                code: crate::data::input::KeyCode::Delete,
                                modifiers: crate::data::input::KeyModifiers::NONE,
                            };
                            if editor.handle_sub_editor_key(tf_key) {
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                        }

                        // Otherwise, delete the whole window
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
                    let tf_key = crate::data::input::KeyEvent { code, modifiers };
                    let ct_code = crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key_event = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    let rt_key =
                        crate::frontend::tui::textarea_bridge::to_textarea_event(key_event);
                    if let Some(ref mut editor) = self.window_editor {
                        if editor.is_sub_editor_active() {
                            if editor.handle_sub_editor_key(tf_key) {
                                app_core.needs_render = true;
                                return Ok(None);
                            }
                        }
                        // Ctrl+P on the Streams field opens the seen-streams
                        // picker (parity with the GUI custom-windows picker).
                        // Seed it here where AppCore is in scope, since the
                        // editor holds no app reference.
                        if modifiers.ctrl
                            && matches!(code, KeyCode::Char('p') | KeyCode::Char('P'))
                            && editor.is_on_streams()
                        {
                            editor.set_seen_streams(app_core.message_processor.seen_streams());
                            editor.open_stream_picker();
                            app_core.needs_render = true;
                            return Ok(None);
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

#[cfg(test)]
mod hit_test_tests {
    use super::{find_topmost_window_at, resize_op_at};
    use crate::data::ui_state::UiState;
    use crate::data::window::WindowState;
    use crate::data::DragOperation;

    /// Insert a visible text window at the given rect.
    fn add_window(ui: &mut UiState, name: &str, x: u16, y: u16, w: u16, h: u16) {
        let mut window = WindowState::new_text(name, 100);
        window.position.x = crate::data::geometry::Col::new(x);
        window.position.y = crate::data::geometry::Row::new(y);
        window.position.width = crate::data::geometry::Width::new(w);
        window.position.height = crate::data::geometry::Height::new(h);
        window.visible = true;
        ui.windows.insert(name.to_string(), window);
    }

    #[test]
    fn topmost_falls_back_to_main_when_empty() {
        let ui = UiState::new();
        assert_eq!(find_topmost_window_at(&ui, 5, 5), "main");
    }

    #[test]
    fn topmost_hits_the_containing_window() {
        let mut ui = UiState::new();
        add_window(&mut ui, "a", 0, 0, 10, 10);
        // Inside a.
        assert_eq!(find_topmost_window_at(&ui, 3, 3), "a");
        // Half-open bounds: the far edges are outside.
        assert_eq!(find_topmost_window_at(&ui, 10, 3), "main");
        assert_eq!(find_topmost_window_at(&ui, 3, 10), "main");
    }

    #[test]
    fn topmost_skips_invisible_windows() {
        let mut ui = UiState::new();
        add_window(&mut ui, "a", 0, 0, 10, 10);
        ui.windows.get_mut("a").unwrap().visible = false;
        assert_eq!(find_topmost_window_at(&ui, 3, 3), "main");
    }

    #[test]
    fn ephemeral_windows_win_over_regular() {
        let mut ui = UiState::new();
        // Overlapping regular + ephemeral at the same point.
        add_window(&mut ui, "regular", 0, 0, 20, 20);
        add_window(&mut ui, "popup", 0, 0, 20, 20);
        ui.ephemeral_windows.insert("popup".to_string());
        assert_eq!(find_topmost_window_at(&ui, 5, 5), "popup");
    }

    #[test]
    fn resize_op_edges_and_corner() {
        // 10x10 window at origin, unlocked. right_col=9, bottom_row=9.
        let op = |x, y| resize_op_at(0, 0, 10, 10, false, x, y);
        assert_eq!(op(9, 9), Some(DragOperation::ResizeBottomRight));
        assert_eq!(op(9, 3), Some(DragOperation::ResizeRight));
        assert_eq!(op(3, 9), Some(DragOperation::ResizeBottom));
        assert_eq!(op(3, 0), Some(DragOperation::Move)); // top row
        assert_eq!(op(3, 3), None); // body
    }

    #[test]
    fn resize_op_locked_only_moves() {
        // Locked: right/bottom/corner are NOT resize handles; only the top row
        // moves. The bottom-right corner (9,9) is neither top row nor a legal
        // handle, so it's a plain body click.
        let op = |x, y| resize_op_at(0, 0, 10, 10, true, x, y);
        assert_eq!(op(9, 9), None);
        assert_eq!(op(9, 3), None); // right column, locked
        assert_eq!(op(3, 9), None); // bottom row, locked
        assert_eq!(op(3, 0), Some(DragOperation::Move)); // top row
    }

    #[test]
    fn resize_op_small_window_bottom_row_is_content() {
        // height <= 2: bottom row is NOT a resize handle (it's the one content
        // row); only the top row moves and the right column resizes width.
        assert_eq!(resize_op_at(0, 0, 10, 2, false, 3, 1), None);
        assert_eq!(
            resize_op_at(0, 0, 10, 2, false, 9, 1),
            Some(DragOperation::ResizeRight)
        );
        assert_eq!(
            resize_op_at(0, 0, 10, 2, false, 3, 0),
            Some(DragOperation::Move)
        );
    }
}
