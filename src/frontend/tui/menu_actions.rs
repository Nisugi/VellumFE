//! Menu Action Handler
//!
//! Processes menu action commands from the TUI.

use crate::config;
use crate::core::AppCore;
use crate::data::ui_state::{InputMode, PopupMenu};
use crate::frontend::tui::menu_builders;
use crate::frontend::tui::TuiFrontend;
use anyhow::Result;

fn close_all_menus(ui_state: &mut crate::data::ui_state::UiState) {
    ui_state.popup_menu = None;
    ui_state.submenu = None;
    ui_state.nested_submenu = None;
}

/// Handle menu action commands
pub fn handle_menu_action(
    app_core: &mut AppCore,
    frontend: &mut TuiFrontend,
    command: &str,
) -> Result<()> {
    if let Some(layout_name) = command.strip_prefix("action:loadlayout:") {
        // Load a layout with proper terminal size
        tracing::info!("[MENU_ACTIONS] Menu action loadlayout: '{}'", layout_name);
        let (width, height) = frontend.size();
        tracing::info!(
            "[MENU_ACTIONS] Terminal size from frontend: {}x{}",
            width,
            height
        );
        if let Some((theme_id, theme)) = app_core.load_layout(layout_name, width, height) {
            frontend.update_theme_cache(theme_id, theme);
        }
    } else if let Some(widget_type) = command.strip_prefix("action:createwindow:") {
        // Create a new window with the specified widget type
        // Safeguard: prevent opening if a window editor is already open
        if frontend.window_editor.is_some() {
            tracing::debug!("Window editor already open, ignoring createwindow request");
        } else if let Some(_template) = config::Config::get_window_template(widget_type) {
            // Open window editor with template (proper defaults + marked as new)
            // Use new_window_with_layout for spacers to enable auto-naming
            frontend.window_editor = Some(
                crate::frontend::tui::window_editor::WindowEditor::new_window_with_layout(
                    widget_type.to_string(),
                    &app_core.layout,
                ),
            );
            app_core.ui_state.input_mode = InputMode::WindowEditor;
        } else {
            tracing::warn!("No template found for widget type: {}", widget_type);
        }
    } else if let Some(window_name) = command.strip_prefix("action:editwindow:") {
        // Edit an existing window
        // Safeguard: prevent opening if a window editor is already open
        if frontend.window_editor.is_some() {
            tracing::debug!("Window editor already open, ignoring editwindow request");
        } else if let Some(window_def) = app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window_name)
            .cloned()
        {
            // Open window editor
            frontend.window_editor = Some(
                crate::frontend::tui::window_editor::WindowEditor::new_with_layout(
                    window_def,
                    &app_core.layout,
                ),
            );
            app_core.ui_state.input_mode = InputMode::WindowEditor;
        } else {
            tracing::warn!("Window not found for editing: {}", window_name);
        }
    } else if let Some(window_name) = command.strip_prefix("action:showwindow:") {
        // Add/show the window (from template)
        // Get terminal size for window positioning
        let (width, height) = frontend.size();

        // Show window from layout template
        app_core.show_window(window_name, width, height);

        // Close menus
        app_core.ui_state.popup_menu = None;
        app_core.ui_state.submenu = None;
        app_core.ui_state.input_mode = InputMode::Normal;
        app_core.needs_render = true;
    } else if let Some(window_name) = command.strip_prefix("action:hidewindow:") {
        // Hide a visible window
        app_core.hide_window(window_name);
    } else if let Some(stream) = command.strip_prefix("action:streamacts:") {
        // Per-stream route actions submenu. Rebuild the streams list under it
        // first: the mouse path closes every menu before dispatching actions.
        let stream = stream.to_string();
        open_streams_menu(app_core, Some(&stream));
        let items = menu_builders::build_stream_actions_menu(app_core, &stream);
        let parent_pos = app_core
            .ui_state
            .popup_menu
            .as_ref()
            .map(|m| m.get_position())
            .unwrap_or((40, 12));
        app_core.ui_state.submenu = Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
    } else if let Some(stream) = command.strip_prefix("action:streamwin:") {
        // "Send to window..." picker (level 3), with its parent menus rebuilt.
        let stream = stream.to_string();
        open_streams_menu(app_core, Some(&stream));
        let parent_pos = app_core
            .ui_state
            .popup_menu
            .as_ref()
            .map(|m| m.get_position())
            .unwrap_or((40, 12));
        let actions = menu_builders::build_stream_actions_menu(app_core, &stream);
        app_core.ui_state.submenu =
            Some(PopupMenu::new(actions, (parent_pos.0 + 2, parent_pos.1)));
        let windows = menu_builders::build_stream_window_menu(app_core, &stream);
        app_core.ui_state.nested_submenu =
            Some(PopupMenu::new(windows, (parent_pos.0 + 4, parent_pos.1)));
    } else if let Some(rest) = command.strip_prefix("action:streamroute:") {
        // Route the stream to main/discard, or clear back to the fallback.
        let (kind, stream) = rest.split_once(':').unwrap_or((rest, ""));
        let route = match kind {
            "main" => Some(Some(crate::config::StreamRoute::Main)),
            "discard" => Some(Some(crate::config::StreamRoute::Discard)),
            "clear" => Some(None),
            _ => None,
        };
        match route {
            Some(route) if !stream.is_empty() => {
                let stream = stream.to_string();
                // Route actions orphan the stream first (a subscription would
                // always win over the route) — same as the GUI Streams panel.
                remove_stream_from_text_windows(app_core, &stream, None);
                if let Err(err) = set_stream_route(app_core, &stream, route) {
                    app_core.add_system_message(&err);
                }
                open_streams_menu(app_core, Some(&stream));
            }
            _ => tracing::warn!("Malformed stream route action: {}", command),
        }
    } else if let Some(rest) = command.strip_prefix("action:streamsub:") {
        // Subscribe an existing text window to the stream (moving it from any
        // other text window). Any route entry stays as the orphan policy.
        if let Some((window, stream)) = rest.split_once(':') {
            let (window, stream) = (window.to_string(), stream.to_string());
            remove_stream_from_text_windows(app_core, &stream, Some(&window));
            if let Err(err) = add_stream_to_text_window(app_core, &window, &stream) {
                app_core.add_system_message(&err);
            }
            open_streams_menu(app_core, Some(&stream));
        } else {
            tracing::warn!("Malformed stream subscribe action: {}", command);
        }
    } else if let Some(stream) = command.strip_prefix("action:streamnew:") {
        // Existing create-custom-window flow with the streams field pre-filled.
        if frontend.window_editor.is_some() {
            tracing::debug!("Window editor already open, ignoring streamnew request");
        } else {
            let mut editor =
                crate::frontend::tui::window_editor::WindowEditor::new_window_with_layout(
                    "text_custom".to_string(),
                    &app_core.layout,
                );
            editor.set_streams_field(stream);
            frontend.window_editor = Some(editor);
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::WindowEditor;
            app_core.needs_render = true;
        }
    } else {
        match command {
            "action:addwindow" => {
                // Close submenu if it exists
                let parent_pos = app_core
                    .ui_state
                    .popup_menu
                    .as_ref()
                    .map(|m| m.get_position())
                    .unwrap_or((40, 12));
                // Show widget category picker as submenu (allows Esc to go back)
                let items = app_core.build_add_window_menu();
                app_core.ui_state.submenu =
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
                app_core.ui_state.nested_submenu = None;
                app_core.ui_state.input_mode = InputMode::Menu;
            }
            "action:hidewindow" => {
                // Close submenu if it exists
                let parent_pos = app_core
                    .ui_state
                    .popup_menu
                    .as_ref()
                    .map(|m| m.get_position())
                    .unwrap_or((40, 12));
                // Show category-based picker for hiding as submenu
                let items = app_core.build_hide_window_menu();
                app_core.ui_state.submenu = if items.is_empty() {
                    None
                } else {
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)))
                };
                app_core.ui_state.nested_submenu = None;
                app_core.ui_state.input_mode = InputMode::Menu;
            }
            "action:listwindows" => {
                // List all windows
                app_core.send_command(".windows".to_string())?;

                // Close menu and return to normal mode
                app_core.ui_state.popup_menu = None;
                app_core.ui_state.input_mode = InputMode::Normal;
                app_core.needs_render = true;
            }
            // "action:editperformance" removed - use right-click on overlay to toggle metrics
            "action:windows" => {
                // List all windows (dot command handled locally)
                app_core.send_command(".windows".to_string())?;

                // Close menu and return to normal mode
                app_core.ui_state.popup_menu = None;
                app_core.ui_state.submenu = None;
                app_core.ui_state.nested_submenu = None;
                app_core.ui_state.input_mode = InputMode::Normal;
                app_core.needs_render = true;
            }
            "action:highlights" => {
                // Open highlight browser with source tracking
                let global_highlights =
                    crate::config::Config::load_common_highlights().unwrap_or_default();
                let character_highlights = crate::config::Config::load_character_highlights_only(
                    app_core.config.character.as_deref(),
                )
                .unwrap_or_default();
                frontend.highlight_browser = Some(
                    crate::frontend::tui::highlight_browser::HighlightBrowser::new_with_source(
                        &global_highlights,
                        &character_highlights,
                    ),
                );
                // Close menus so focus goes to the browser
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::HighlightBrowser;
            }
            "action:addhighlight" => {
                // Open highlight form for creating new highlight
                let mut form = crate::frontend::tui::highlight_form::HighlightFormWidget::new();
                form.set_rumble_options(app_core.config.controller_rumble.pattern_names());
                frontend.highlight_form = Some(form);
                // Close menus so only the form remains
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::HighlightForm;
            }
            "action:keybinds" => {
                // Open keybind browser with source tracking ([G]/[C] indicators)
                // Load global and character keybinds separately to show their origin
                let global_keybinds =
                    crate::config::Config::load_common_keybinds().unwrap_or_default();
                let character_keybinds = crate::config::Config::load_character_keybinds_only(
                    app_core.config.character.as_deref(),
                )
                .unwrap_or_default();

                frontend.keybind_browser = Some(
                    crate::frontend::tui::keybind_browser::KeybindBrowser::new_with_source(
                        &global_keybinds,
                        &character_keybinds,
                    ),
                );
                // Close menus so focus moves to the browser
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::KeybindBrowser;
            }
            "action:streams" => {
                // Open the streams routing menu (every known stream and where
                // it goes) — the TUI mirror of the GUI Streams panel.
                close_all_menus(&mut app_core.ui_state);
                open_streams_menu(app_core, None);
            }
            "action:hotbars" => {
                // Open the hotbar editor (bars -> buttons -> button form)
                frontend.hotbar_editor = Some(
                    crate::frontend::tui::hotbar_editor::HotbarEditor::new(&app_core.config),
                );
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::HotbarEditor;
            }
            "action:addkeybind" => {
                // Open keybind form for creating new keybind
                frontend.keybind_form =
                    Some(crate::frontend::tui::keybind_form::KeybindFormWidget::new());
                // Close menus so only the form remains
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::KeybindForm;
            }
            "action:colors" => {
                // Open color palette browser with source tracking for [G]/[C] indicators
                let global_colors = match crate::config::ColorConfig::load_common_colors() {
                    Ok(c) => c.color_palette,
                    Err(_) => Vec::new(),
                };
                let character_colors = match crate::config::ColorConfig::load_character_colors_only(
                    app_core.config.character.as_deref(),
                ) {
                    Ok(c) => c.color_palette,
                    Err(_) => Vec::new(),
                };
                frontend.color_palette_browser = Some(
                    crate::frontend::tui::color_palette_browser::ColorPaletteBrowser::new_with_source(
                        &global_colors,
                        &character_colors,
                    ),
                );
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::ColorPaletteBrowser;
            }
            "action:addcolor" => {
                // Open color form for creating new palette color
                frontend.color_form =
                    Some(crate::frontend::tui::color_form::ColorForm::new_create());
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::ColorForm;
            }
            "action:uicolors" => {
                // Open UI colors browser
                frontend.uicolors_browser = Some(
                    crate::frontend::tui::uicolors_browser::UIColorsBrowser::new(
                        &app_core.config.colors,
                    ),
                );
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::UIColorsBrowser;
            }
            "action:spellcolors" => {
                // Open spell colors browser
                frontend.spell_color_browser = Some(
                    crate::frontend::tui::spell_color_browser::SpellColorBrowser::new(
                        &app_core.config.colors.spell_colors,
                    ),
                );
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::SpellColorsBrowser;
            }
            "action:addspellcolor" => {
                // Open spell color form for creating new spell color
                frontend.spell_color_form =
                    Some(crate::frontend::tui::spell_color_form::SpellColorFormWidget::new());
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::SpellColorForm;
            }
            "action:settings" => {
                // Open settings editor with source tracking
                // Check if character-specific config exists to determine setting sources
                let character_config_exists = crate::config::Config::load_character_config_only(
                    app_core.config.character.as_deref(),
                )
                .ok()
                .flatten()
                .is_some();

                let settings_items = menu_builders::build_settings_items_with_source(
                    &app_core.config,
                    character_config_exists,
                );
                frontend.settings_editor = Some(
                    crate::frontend::tui::settings_editor::SettingsEditor::new(settings_items),
                );
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::SettingsEditor;
            }
            "action:themes" => {
                // Open theme browser (includes built-in and custom themes)
                frontend.theme_browser =
                    Some(crate::frontend::tui::theme_browser::ThemeBrowser::new(
                        app_core.config.active_theme.clone(),
                        app_core.config.character.as_deref(),
                    ));
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::ThemeBrowser;
            }
            action if action.starts_with("action:settheme:") => {
                // Update frontend theme cache when theme changes via .settheme command
                let theme_id = action.strip_prefix("action:settheme:").unwrap().to_string();
                let theme = app_core.config.get_theme();
                frontend.update_theme_cache(theme_id, theme);
                app_core.needs_render = true;
            }
            action
                if action == "action:skins"
                    || action == "action:reloadskin"
                    || action.starts_with("action:setskin:")
                    || action.starts_with("action:makeskin:") =>
            {
                // Skins are image-based GUI decoration; the terminal frontend
                // has no image pipeline, so just point the user at the GUI.
                app_core.add_system_message(
                    "Skins apply to the GUI frontend. Start with --frontend gui to use them.",
                );
                app_core.needs_render = true;
            }
            "action:edittheme" => {
                // Open theme editor with current theme
                let current_theme = app_core.config.get_theme();
                frontend.theme_editor = Some(
                    crate::frontend::tui::theme_editor::ThemeEditor::new_edit(&current_theme),
                );
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::ThemeEditor;
            }
            "action:editwindow" => {
                let parent_pos = app_core
                    .ui_state
                    .popup_menu
                    .as_ref()
                    .map(|m| m.get_position())
                    .unwrap_or((40, 12));
                // Show category-based picker for editing as submenu
                let items = app_core.build_edit_window_menu();
                app_core.ui_state.submenu = if items.is_empty() {
                    None
                } else {
                    Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)))
                };
                app_core.ui_state.nested_submenu = None;
                app_core.ui_state.input_mode = InputMode::Menu;
            }
            "action:nexttab" => {
                // Navigate to next tab in all tabbed windows
                frontend.next_tab_all();
                frontend.sync_tabbed_active_state(app_core);
                app_core.needs_render = true;
            }
            "action:prevtab" => {
                // Navigate to previous tab in all tabbed windows
                frontend.prev_tab_all();
                frontend.sync_tabbed_active_state(app_core);
                app_core.needs_render = true;
            }
            "action:gonew" => {
                // Navigate to next tab with unread messages
                if !frontend.go_to_next_unread_tab() {
                    app_core.add_system_message("No tabs with new messages");
                }
                frontend.sync_tabbed_active_state(app_core);
                app_core.needs_render = true;
            }
            "action:setpalette" => {
                // Load color_palette colors into terminal palette slots using OSC 4
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::Normal;
                if let Err(e) = frontend.execute_setpalette(app_core) {
                    app_core.add_system_message(&format!("Failed to set palette: {}", e));
                } else {
                    let count = app_core
                        .config
                        .colors
                        .color_palette
                        .iter()
                        .filter(|c| c.slot.is_some())
                        .count();
                    app_core.add_system_message(&format!(
                        "Loaded {} colors into terminal palette",
                        count
                    ));
                }
                app_core.needs_render = true;
            }
            "action:resetpalette" => {
                // Reset terminal palette to defaults using OSC 104
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::Normal;
                if let Err(e) = frontend.execute_resetpalette() {
                    app_core.add_system_message(&format!("Failed to reset palette: {}", e));
                } else {
                    app_core.add_system_message("Terminal palette reset to defaults");
                }
                app_core.needs_render = true;
            }
            _ => {
                tracing::warn!("Unknown menu action: {}", command);
            }
        }
    }
    Ok(())
}

// ---- Streams routing helpers (.streams menu) ------------------------------
//
// These mirror the GUI Streams panel's semantics: subscription edits sync the
// layout definition (or they are lost on restart) and rebuild the routing
// cache; route edits persist through the sparse config save and are pushed
// into the live message processor.

/// Rebuild and show the streams list menu (level 1), optionally keeping the
/// selection on one stream. Deeper levels are cleared; callers reopen them.
fn open_streams_menu(app_core: &mut AppCore, select: Option<&str>) {
    let items = menu_builders::build_streams_menu(app_core);
    let mut menu = PopupMenu::new(items, (40, 12));
    if let Some(stream) = select {
        let target = format!("action:streamacts:{}", stream);
        if let Some(idx) = menu.items.iter().position(|item| item.command == target) {
            menu.selected = idx;
        }
    }
    app_core.ui_state.popup_menu = Some(menu);
    app_core.ui_state.submenu = None;
    app_core.ui_state.nested_submenu = None;
    app_core.ui_state.input_mode = InputMode::Menu;
    app_core.needs_render = true;
}

/// Mirror a text window's live stream list into its layout definition so the
/// change survives a restart (same fix the Window Editor carries).
fn sync_text_streams_to_layout(app_core: &mut AppCore, name: &str, streams: Vec<String>) {
    if let Some(crate::config::WindowDef::Text { data, .. }) = app_core
        .layout
        .windows
        .iter_mut()
        .find(|w| w.name() == name)
    {
        data.streams = streams;
        app_core.layout_modified_since_save = true;
    }
}

/// Remove `stream` from every plain Text window's stream list except `keep`,
/// syncing layout definitions and the routing cache.
fn remove_stream_from_text_windows(app_core: &mut AppCore, stream: &str, keep: Option<&str>) {
    let mut changed: Vec<(String, Vec<String>)> = Vec::new();
    for (name, window) in app_core.ui_state.windows.iter_mut() {
        if keep == Some(name.as_str()) {
            continue;
        }
        // Only plain Text widgets: tabbed tabs and built-in inventory-type
        // widgets are Window Editor territory (their rows are read-only here).
        let crate::data::WindowContent::Text(text) = &mut window.content else {
            continue;
        };
        let before = text.streams.len();
        text.streams
            .retain(|s| !s.trim().eq_ignore_ascii_case(stream));
        if text.streams.len() != before {
            changed.push((name.clone(), text.streams.clone()));
        }
    }
    if changed.is_empty() {
        return;
    }
    for (name, streams) in changed {
        sync_text_streams_to_layout(app_core, &name, streams);
    }
    app_core
        .message_processor
        .update_text_stream_subscribers(&app_core.ui_state);
    app_core.needs_render = true;
}

/// Subscribe an existing Text window to `stream` (no-op when already
/// subscribed), syncing the layout definition and the routing cache.
fn add_stream_to_text_window(
    app_core: &mut AppCore,
    name: &str,
    stream: &str,
) -> Result<(), String> {
    let streams = {
        let window = app_core
            .ui_state
            .windows
            .get_mut(name)
            .ok_or_else(|| format!("Window '{}' no longer exists.", name))?;
        let crate::data::WindowContent::Text(text) = &mut window.content else {
            return Err(format!("Window '{}' is not a text window.", name));
        };
        if !text
            .streams
            .iter()
            .any(|s| s.trim().eq_ignore_ascii_case(stream))
        {
            text.streams.push(stream.to_string());
        }
        text.streams.clone()
    };
    sync_text_streams_to_layout(app_core, name, streams);
    app_core
        .message_processor
        .update_text_stream_subscribers(&app_core.ui_state);
    app_core.needs_render = true;
    Ok(())
}

/// Set (or clear, with `None`) the `[streams.routes]` entry for a stream,
/// persist it via the sparse profile save, and push the new config into the
/// message processor (which routes from its own copy).
fn set_stream_route(
    app_core: &mut AppCore,
    stream: &str,
    route: Option<crate::config::StreamRoute>,
) -> Result<(), String> {
    let routes = &mut app_core.config.streams.routes;
    // Replace any existing entry regardless of letter case (route lookup is
    // case-insensitive, so near-duplicates would shadow each other).
    routes.retain(|id, _| !id.eq_ignore_ascii_case(stream));
    if let Some(route) = route {
        routes.insert(stream.to_string(), route);
    }
    app_core
        .save_config()
        .map_err(|err| format!("Failed to save config: {}", err))?;
    let config = app_core.config.clone();
    app_core.message_processor.apply_config(config);
    app_core.needs_render = true;
    Ok(())
}
