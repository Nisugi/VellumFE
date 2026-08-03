//! Menu Action Handler
//!
//! Processes menu action commands from the TUI.

use crate::config;
use crate::core::AppCore;
use crate::data::ui_state::{InputMode, PopupMenu};
use crate::data::UiAction;
use crate::frontend::tui::menu_builders;
use crate::frontend::tui::TuiFrontend;
use anyhow::Result;

fn close_all_menus(ui_state: &mut crate::data::ui_state::UiState) {
    ui_state.popup_menu = None;
    ui_state.submenu = None;
    ui_state.nested_submenu = None;
}

/// Handle a menu/action command in its string form (popup-menu items
/// carry strings). The typed path is [`handle_ui_action`]; this is the
/// single bridge from strings into it.
pub fn handle_menu_action(
    app_core: &mut AppCore,
    frontend: &mut TuiFrontend,
    command: &str,
) -> Result<()> {
    match UiAction::parse(command) {
        Some(action) => handle_ui_action(app_core, frontend, action),
        None => {
            // Menus only feed action strings here; an unparseable one is
            // a menu-wiring bug — tell the user instead of logging into
            // the void (the old behavior that hid four dead commands).
            tracing::warn!("Unknown menu action: {}", command);
            app_core.add_system_message(&format!("Unknown action: {command}"));
            app_core.needs_render = true;
            Ok(())
        }
    }
}

/// A UI action this frontend deliberately does not implement: say so.
fn gui_only(app_core: &mut AppCore, what: &str) {
    app_core.add_system_message(&format!(
        "{what} needs the GUI frontend (start with --frontend gui)."
    ));
    app_core.needs_render = true;
}

/// Perform a [`UiAction`] in the TUI. The match is EXHAUSTIVE on
/// purpose: adding a UiAction variant forces every frontend to decide —
/// implement it or route it through [`gui_only`] — so actions can never
/// silently die again.
pub fn handle_ui_action(
    app_core: &mut AppCore,
    frontend: &mut TuiFrontend,
    action: UiAction,
) -> Result<()> {
    match action {
        // The Layouts menu names TOML layouts explicitly; `.loadlayout`
        // resolves to the same thing in this frontend.
        // The TUI has no skins; `keep_skin` is a GUI-only concern and is
        // accepted-and-ignored here.
        UiAction::LoadLayoutToml(layout_name)
        | UiAction::LoadLayout {
            name: Some(layout_name),
            ..
        } => {
            // Load a layout with proper terminal size
            tracing::info!("[MENU_ACTIONS] Menu action loadlayout: '{}'", layout_name);
            let (width, height) = frontend.size();
            tracing::info!(
                "[MENU_ACTIONS] Terminal size from frontend: {}x{}",
                width,
                height
            );
            if let Some((theme_id, theme)) = app_core.load_layout(&layout_name, width, height) {
                frontend.update_theme_cache(theme_id, theme);
            }
            app_core.needs_render = true;
        }
        UiAction::LoadLayout { name: None, .. } => {
            // Bare `.loadlayout` shows usage + the saved list, matching the
            // GUI (it used to load 'default', which silently replaced the
            // current arrangement on a bare invocation).
            app_core.add_system_message("Usage: .loadlayout <name>");
            app_core.list_layouts();
            app_core.needs_render = true;
        }
        UiAction::SaveLayout(name) => {
            let name = name.unwrap_or_else(|| "default".to_string());
            let (width, height) = frontend.size();
            app_core.save_layout(&name, width, height);
            app_core.needs_render = true;
        }
        UiAction::ListLayouts => {
            app_core.list_layouts();
            app_core.needs_render = true;
        }
        UiAction::ResizeLayout(None) => {
            let (width, height) = frontend.size();
            app_core.resize_windows(width, height);
            app_core.needs_render = true;
        }
        UiAction::ResizeLayout(Some(_)) => {
            // The TUI reflows its cell grid natively; adopting a saved
            // layout's pixel geometry is a GUI concept.
            gui_only(
                app_core,
                "Adopting a saved layout's geometry (.resize <name>)",
            )
        }
        UiAction::SaveSkin(_) => {
            gui_only(app_core, "Saving a skin from the current appearance (.saveskin)")
        }
        UiAction::Reconnect => {
            // The runtime loop owns the network channels; flag it and let
            // the next tick do the actual reconnect.
            app_core.reconnect_requested = true;
            app_core.needs_render = true;
        }
        UiAction::UiExport(args) => {
            // The plain core pack; the GUI adds its live layout on top.
            app_core.uiexport_with(&args, Vec::new());
            app_core.needs_render = true;
        }
        UiAction::UiImport(args) => {
            if app_core.uiimport(&args).is_some() {
                app_core.add_system_message(
                    "This pack also carries a GUI layout — run the import in the GUI to install it.",
                );
            }
            app_core.needs_render = true;
        }
        UiAction::CreateWindow(widget_type) => {
            // Create a new window with the specified widget type
            // Safeguard: prevent opening if a window editor is already open
            if frontend.window_editor.is_some() {
                tracing::debug!("Window editor already open, ignoring createwindow request");
            } else if let Some(_template) = config::Config::get_window_template(&widget_type) {
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
        }
        UiAction::EditWindow(Some(window_name)) => {
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
        }
        UiAction::EditWindow(None) => {
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
        UiAction::ShowWindow(window_name) => {
            // Add/show the window (from template)
            // Get terminal size for window positioning
            let (width, height) = frontend.size();

            // Show window from layout template
            app_core.show_window(&window_name, width, height);

            // Close menus
            app_core.ui_state.popup_menu = None;
            app_core.ui_state.submenu = None;
            app_core.ui_state.input_mode = InputMode::Normal;
            app_core.needs_render = true;
        }
        UiAction::HideWindow(Some(window_name)) => {
            // Hide a visible window
            app_core.hide_window(&window_name);
        }
        UiAction::HideWindow(None) => {
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
        UiAction::EditHighlight(Some(name)) => {
            // `.edithighlight <name>`: open the edit form for that highlight.
            match app_core.config.highlights.get(&name) {
                Some(pattern) => {
                    let mut form =
                        crate::frontend::tui::highlight_form::HighlightFormWidget::new_edit(
                            name.to_string(),
                            pattern,
                        );
                    form.set_rumble_options(app_core.config.controller_rumble.pattern_names());
                    frontend.highlight_form = Some(form);
                    close_all_menus(&mut app_core.ui_state);
                    app_core.ui_state.input_mode = InputMode::HighlightForm;
                }
                None => {
                    app_core.add_system_message(&format!(
                        "No highlight named '{}' (.highlights lists them)",
                        name
                    ));
                    app_core.needs_render = true;
                }
            }
        }
        // Bare `.edithighlight` opens the browser to pick from, same as
        // `.highlights` (the GUI does the equivalent).
        UiAction::Highlights | UiAction::EditHighlight(None) => {
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
        UiAction::StreamActions(stream) => {
            // Per-stream route actions submenu. Rebuild the streams list under it
            // first: the mouse path closes every menu before dispatching actions.
            open_streams_menu(app_core, Some(&stream));
            let items = menu_builders::build_stream_actions_menu(app_core, &stream);
            let parent_pos = app_core
                .ui_state
                .popup_menu
                .as_ref()
                .map(|m| m.get_position())
                .unwrap_or((40, 12));
            app_core.ui_state.submenu =
                Some(PopupMenu::new(items, (parent_pos.0 + 2, parent_pos.1)));
        }
        UiAction::StreamPickWindow(stream) => {
            // "Send to window..." picker (level 3), with its parent menus rebuilt.
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
        }
        UiAction::StreamRoute { kind, stream } => {
            // Route the stream to main/discard, or clear back to the fallback.
            let route = match kind.as_str() {
                "main" => Some(Some(crate::config::StreamRoute::Main)),
                "discard" => Some(Some(crate::config::StreamRoute::Discard)),
                "clear" => Some(None),
                _ => None,
            };
            match route {
                Some(route) if !stream.is_empty() => {
                    // Route actions orphan the stream first (a subscription would
                    // always win over the route) — same as the GUI Streams panel.
                    app_core.remove_stream_from_text_windows(&stream, None);
                    if let Err(err) = app_core.set_stream_route(&stream, route) {
                        app_core.add_system_message(&err);
                    }
                    open_streams_menu(app_core, Some(&stream));
                }
                _ => tracing::warn!("Malformed stream route action: {kind}:{stream}"),
            }
        }
        UiAction::StreamSubscribe { window, stream } => {
            // Subscribe an existing text window to the stream (moving it from any
            // other text window). Any route entry stays as the orphan policy.
            app_core.remove_stream_from_text_windows(&stream, Some(&window));
            if let Err(err) = app_core.add_stream_to_text_window(&window, &stream) {
                app_core.add_system_message(&err);
            }
            open_streams_menu(app_core, Some(&stream));
        }
        UiAction::StreamNewWindow(stream) => {
            // Existing create-custom-window flow with the streams field pre-filled.
            if frontend.window_editor.is_some() {
                tracing::debug!("Window editor already open, ignoring streamnew request");
            } else {
                let mut editor =
                    crate::frontend::tui::window_editor::WindowEditor::new_window_with_layout(
                        "text_custom".to_string(),
                        &app_core.layout,
                    );
                editor.set_streams_field(&stream);
                frontend.window_editor = Some(editor);
                close_all_menus(&mut app_core.ui_state);
                app_core.ui_state.input_mode = InputMode::WindowEditor;
                app_core.needs_render = true;
            }
        }
        UiAction::AddWindowPicker => {
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
        UiAction::WindowList => {
            // List all windows (one arm for both historical spellings)
            app_core.send_command(".windows".to_string())?;

            // Close menu and return to normal mode
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::Normal;
            app_core.needs_render = true;
        }
        UiAction::AddHighlight => {
            // Open highlight form for creating new highlight
            let mut form = crate::frontend::tui::highlight_form::HighlightFormWidget::new();
            form.set_rumble_options(app_core.config.controller_rumble.pattern_names());
            frontend.highlight_form = Some(form);
            // Close menus so only the form remains
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::HighlightForm;
        }
        UiAction::Keybinds => {
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
        UiAction::MenuKeybinds => {
            // Open the menu keybind editor (the fixed [menu] nav/action
            // keys). Defaults to global scope; toggle with 'g' inside.
            frontend.menu_keybind_editor = Some(
                crate::frontend::tui::menu_keybind_editor::MenuKeybindEditor::new(
                    app_core.config.menu_keybinds.clone(),
                    true,
                ),
            );
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::MenuKeybindEditor;
        }
        UiAction::EditStatusAbbrev => {
            // Standalone editor for the global target_list.status_abbrev map
            // (full status name -> short tag), seeded from live config.
            frontend.status_abbrev_editor = Some(
                crate::frontend::tui::status_abbrev_editor::StatusAbbrevEditor::new(
                    &app_core.config.target_list.status_abbrev,
                ),
            );
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::StatusAbbrevEditor;
        }
        // The GUI's Streams & Custom Windows panel and the TUI's
        // `.streams` menu are the same surface.
        UiAction::Streams | UiAction::CustomWindows => {
            // Open the streams routing menu (every known stream and where
            // it goes) — the TUI mirror of the GUI Streams panel.
            close_all_menus(&mut app_core.ui_state);
            open_streams_menu(app_core, None);
        }
        UiAction::KnownWindows => {
            // The GUI's consolidated Windows manager; the TUI equivalent
            // is the Show/Hide pickers on the main menu.
            app_core.add_system_message(
                "Use .menu > Windows for the TUI's show/hide list (the Windows manager is a GUI window).",
            );
            app_core.needs_render = true;
        }
        UiAction::EditIndicators => {
            // The indicator template builder: create/edit every status
            // indicator, its conditions, and condition-driven icons in one
            // place. Same editor the `Indicators > Editor` leaf opens, now a
            // first-class action so it is reachable even with every indicator
            // already placed.
            frontend.indicator_template_editor = Some(
                crate::frontend::tui::indicator_template_editor::IndicatorTemplateEditor::new(),
            );
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::IndicatorTemplateEditor;
            app_core.needs_render = true;
        }
        UiAction::Hotbars => {
            // Open the hotbar editor (bars -> buttons -> button form)
            frontend.hotbar_editor = Some(
                crate::frontend::tui::hotbar_editor::HotbarEditor::new(&app_core.config),
            );
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::HotbarEditor;
        }
        UiAction::AddKeybind => {
            // Open keybind form for creating new keybind
            frontend.keybind_form =
                Some(crate::frontend::tui::keybind_form::KeybindFormWidget::new());
            // Close menus so only the form remains
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::KeybindForm;
        }
        UiAction::Colors => {
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
        UiAction::AddColor => {
            // Open color form for creating new palette color
            frontend.color_form =
                Some(crate::frontend::tui::color_form::ColorForm::new_create());
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::ColorForm;
        }
        UiAction::UiColors => {
            // Open UI colors browser
            frontend.uicolors_browser = Some(
                crate::frontend::tui::uicolors_browser::UIColorsBrowser::new(
                    &app_core.config.colors,
                ),
            );
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::UIColorsBrowser;
        }
        UiAction::SpellColors => {
            // Open spell colors browser
            frontend.spell_color_browser = Some(
                crate::frontend::tui::spell_color_browser::SpellColorBrowser::new(
                    &app_core.config.colors.spell_colors,
                ),
            );
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::SpellColorsBrowser;
        }
        UiAction::AddSpellColor => {
            // Open spell color form for creating new spell color
            frontend.spell_color_form =
                Some(crate::frontend::tui::spell_color_form::SpellColorFormWidget::new());
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::SpellColorForm;
        }
        UiAction::Settings => {
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
        UiAction::PackEditor => {
            match crate::config::Config::base_dir() {
                Ok(base) => {
                    frontend.pack_editor = Some(
                        crate::frontend::tui::pack_editor::PackEditorWidget::new(base),
                    );
                    close_all_menus(&mut app_core.ui_state);
                    app_core.ui_state.input_mode = InputMode::PackEditor;
                }
                Err(e) => {
                    app_core.add_system_message(&format!("Pack editor unavailable: {e}"))
                }
            }
            app_core.needs_render = true;
        }
        UiAction::Themes => {
            // Open theme browser (includes built-in and custom themes)
            frontend.theme_browser =
                Some(crate::frontend::tui::theme_browser::ThemeBrowser::new(
                    app_core.config.active_theme.clone(),
                    app_core.config.character.as_deref(),
                ));
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::ThemeBrowser;
        }
        UiAction::SetTheme(theme_id) => {
            // Validate the requested theme, set it as active, save, THEN fetch
            // it. Previously this fetched get_theme() (which reads the CURRENT
            // active_theme) and cached that unchanged theme under the new id —
            // so `.settheme X` was a no-op that never switched anything.
            let presets = crate::theme::ThemePresets::all_with_custom(
                app_core.config.character.as_deref(),
            );
            if !presets.contains_key(&theme_id) {
                let mut names: Vec<&String> = presets.keys().collect();
                names.sort();
                let list = names.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ");
                app_core.add_system_message(&format!(
                    "Unknown theme '{}'. Available: {}",
                    theme_id, list
                ));
            } else {
                app_core.config.active_theme = theme_id.clone();
                if let Err(err) = app_core.config.save(app_core.config.character.as_deref()) {
                    tracing::warn!("Failed to save config after theme switch: {}", err);
                }
                let theme = app_core.config.get_theme();
                frontend.update_theme_cache(theme_id, theme);
                app_core.needs_render = true;
            }
        }
        UiAction::EditTheme => {
            // Open theme editor with current theme
            let current_theme = app_core.config.get_theme();
            frontend.theme_editor = Some(
                crate::frontend::tui::theme_editor::ThemeEditor::new_edit(&current_theme),
            );
            close_all_menus(&mut app_core.ui_state);
            app_core.ui_state.input_mode = InputMode::ThemeEditor;
        }
        UiAction::Skins
        | UiAction::SetSkin(_)
        | UiAction::MakeSkin(_)
        | UiAction::HarmonySkin(_)
        | UiAction::ReloadSkin => {
            // Skins are image-based GUI decoration; the terminal frontend
            // has no image pipeline, so just point the user at the GUI.
            app_core.add_system_message(
                "Skins apply to the GUI frontend. Start with --frontend gui to use them.",
            );
            app_core.needs_render = true;
        }
        UiAction::NextTab => {
            // Navigate to next tab in all tabbed windows
            frontend.next_tab_all();
            frontend.sync_tabbed_active_state(app_core);
            app_core.needs_render = true;
        }
        UiAction::PrevTab => {
            // Navigate to previous tab in all tabbed windows
            frontend.prev_tab_all();
            frontend.sync_tabbed_active_state(app_core);
            app_core.needs_render = true;
        }
        UiAction::NextUnread => {
            // Navigate to next tab with unread messages
            if !frontend.go_to_next_unread_tab() {
                app_core.add_system_message("No tabs with new messages");
            }
            frontend.sync_tabbed_active_state(app_core);
            app_core.needs_render = true;
        }
        UiAction::SetPalette => {
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
        UiAction::ResetPalette => {
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
        UiAction::TouchWheelEditor => {
            // The touch wheel is the phone's long-press ring; it's edited
            // from the phone or the GUI, which both write the shared config.
            app_core.add_system_message(
                "The touch wheel is the phone's long-press radial wheel. \
                 Edit it from the phone (Settings > Touch wheel) or the \
                 desktop GUI — both save to the same config.",
            );
            app_core.needs_render = true;
        }
        UiAction::SorterEdit => {
            // TUI parity for the structured editor (rules/order/renames)
            // is planned; the scalar toggles already ride the registry.
            app_core.add_system_message(
                "The sorter rules editor is GUI-only for now. The on/off, \
                 counts, bold-label, and item-order toggles are in \
                 .settings under UI (Sorter rows).",
            );
            app_core.needs_render = true;
        }
        // Deliberately GUI-only surfaces — say so instead of the old
        // silent log (four commands died unnoticed behind that silence).
        UiAction::Controller => gui_only(app_core, "The controller editor"),
        UiAction::JinxPanel => {
            gui_only(app_core, "The Jinx asset panel (.jinx gui) — use .jinx list/install in the TUI")
        }
        UiAction::WebUiPicker | UiAction::WebUiOff | UiAction::WebUiOpen(_) => {
            gui_only(app_core, "The Lich WebUI bridge (.webui)")
        }
        UiAction::Zone { .. } => {
            gui_only(app_core, "Shell zones (.header/.footer/.leftbar/.rightbar)")
        }
        UiAction::SnapDebug => gui_only(app_core, "Snap diagnostics (.snapdebug)"),
        UiAction::PerformanceDump => {
            app_core.write_perf_dump(crate::performance::PerfFrontend::Tui, None);
            app_core.needs_render = true;
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

// The stream-routing mutations themselves (subscribe, orphan, route,
// layout sync) live on AppCore (core/app_core/streams.rs), shared with
// the GUI's Streams & Custom Windows panel; only the menu plumbing above
// is TUI-specific.
