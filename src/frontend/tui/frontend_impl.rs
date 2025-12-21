use anyhow::Result;
use crate::core::AppCore;
use crate::frontend::{Frontend, FrontendEvent};
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use std::time::Instant;
use super::*;

impl Frontend for TuiFrontend {
    fn poll_events(&mut self) -> Result<Vec<FrontendEvent>> {
        let mut events = Vec::new();

        // Poll for events (non-blocking)
        if event::poll(std::time::Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    // Only process key press events, not release events
                    if key.kind == KeyEventKind::Press {
                        if let Some(code) = crossterm_bridge::convert_keycode(key.code) {
                            events.push(FrontendEvent::Key {
                                code,
                                modifiers: crossterm_bridge::convert_modifiers(key.modifiers),
                            });
                        }
                    }
                }
                Event::Resize(width, height) => {
                    // Apply resize debouncing to prevent excessive layout recalculations
                    if let Some((w, h)) = self.resize_debouncer.check_resize(width, height) {
                        events.push(FrontendEvent::Resize { width: w, height: h });
                    }
                }
                Event::Mouse(mouse) => {
                    // Convert crossterm MouseEvent to frontend-agnostic MouseEvent
                    if let Some(kind) = crossterm_bridge::convert_mouse_kind(mouse.kind) {
                        let modifiers = crossterm_bridge::convert_modifiers(mouse.modifiers);
                        let mouse_event = crate::frontend::common::MouseEvent::new(
                            kind,
                            mouse.column,
                            mouse.row,
                            modifiers,
                        );
                        events.push(FrontendEvent::Mouse(mouse_event));
                    }
                }
                Event::Paste(text) => {
                    events.push(FrontendEvent::Paste { text });
                }
                _ => {}
            }
        }

        // Check for pending resize (if debounce period has passed)
        if let Some((width, height)) = self.resize_debouncer.check_pending() {
            events.push(FrontendEvent::Resize { width, height });
        }

        Ok(events)
    }

    fn render(&mut self, app: &mut dyn std::any::Any) -> Result<()> {
        // Downcast to AppCore
        let app_core = app
            .downcast_mut::<AppCore>()
            .ok_or_else(|| anyhow::anyhow!("Invalid app type"))?;

        // Clone theme once so all sync tasks share the same palette
        let theme = self.theme_cache.get_theme().clone();

        // Sync data from data layer into TextWindows
        self.sync_text_windows(app_core, &theme);

        // Sync CommandInput widget configuration from layout
        self.sync_command_inputs(app_core, &theme);

        // Sync room window data from AppCore
        self.sync_room_windows(app_core, &theme);

        // Sync inventory window data from AppCore
        self.sync_inventory_windows(app_core, &theme);

        // Sync spells window data from AppCore
        self.sync_spells_windows(app_core, &theme);

        // Sync perception window data from AppCore
        self.sync_perception_windows(app_core, &theme);

        // Sync progress bar data from AppCore
        self.sync_progress_bars(app_core, &theme);
        self.sync_countdowns(app_core, &theme);
        self.sync_active_effects(app_core, &theme);
        self.sync_hand_widgets(app_core, &theme);
        self.sync_spacer_widgets(app_core, &theme);
        self.sync_indicator_widgets(app_core, &theme);
        self.sync_targets_widgets(app_core, &theme);
        self.sync_players_widgets(app_core, &theme);
        self.sync_dashboard_widgets(app_core, &theme);
        self.sync_tabbed_text_windows(app_core, &theme);
        self.sync_compass_widgets(app_core, &theme);
        self.sync_injury_doll_widgets(app_core, &theme);
        self.sync_performance_widgets(app_core, &theme);

        // Temporarily take ownership of widgets to use in render
        let mut text_windows = std::mem::take(&mut self.widget_manager.text_windows);
        let command_inputs = std::mem::take(&mut self.widget_manager.command_inputs);
        let mut room_windows = std::mem::take(&mut self.widget_manager.room_windows);
        let mut inventory_windows = std::mem::take(&mut self.widget_manager.inventory_windows);
        let mut spells_windows = std::mem::take(&mut self.widget_manager.spells_windows);
        let mut perception_windows = std::mem::take(&mut self.widget_manager.perception_windows);
        let mut progress_bars = std::mem::take(&mut self.widget_manager.progress_bars);
        let mut countdowns = std::mem::take(&mut self.widget_manager.countdowns);
        let mut active_effects_windows = std::mem::take(&mut self.widget_manager.active_effects_windows);
        let mut hand_widgets = std::mem::take(&mut self.widget_manager.hand_widgets);
        let mut spacer_widgets = std::mem::take(&mut self.widget_manager.spacer_widgets);
        let mut indicator_widgets = std::mem::take(&mut self.widget_manager.indicator_widgets);
        let mut targets_widgets = std::mem::take(&mut self.widget_manager.targets_widgets);
        let mut players_widgets = std::mem::take(&mut self.widget_manager.players_widgets);
        let mut dashboard_widgets = std::mem::take(&mut self.widget_manager.dashboard_widgets);
        let mut tabbed_text_windows = std::mem::take(&mut self.widget_manager.tabbed_text_windows);
        let mut compass_widgets = std::mem::take(&mut self.widget_manager.compass_widgets);
        let mut injury_doll_widgets = std::mem::take(&mut self.widget_manager.injury_doll_widgets);
        let mut performance_widgets = std::mem::take(&mut self.widget_manager.performance_widgets);

        // Clone cached theme for use in render closure (cheaper than HashMap lookup + clone per widget)
        let theme_for_render = theme.clone();

        let render_start = Instant::now();

        self.terminal.draw(|f| {
            use crate::data::WindowContent;
            use ratatui::layout::Rect;
            use ratatui::style::{Color, Style};
            use ratatui::text::{Line, Span};
            use ratatui::widgets::{Block, Borders, Paragraph};

            let theme = theme_for_render.clone();
            let screen_area = f.area();

            // Stable render order: sort by name, but draw the performance overlay last so it sits on top
            let mut window_order: Vec<&String> = app_core.ui_state.windows.keys().collect();
            window_order.sort();
            if let Some(pos) = window_order.iter().position(|n| n.as_str() == "performance_overlay") {
                let overlay = window_order.remove(pos);
                window_order.push(overlay);
            }
            let window_index_map: std::collections::HashMap<&String, usize> = window_order
                .iter()
                .enumerate()
                .map(|(idx, name)| (*name, idx))
                .collect();

            // Render each window at its position
            for name in window_order {
                let Some(window) = app_core.ui_state.windows.get(name) else {
                    continue;
                };
                if !window.visible {
                    continue;
                }

                let pos = &window.position;
                let area = Rect {
                    x: pos.x,
                    y: pos.y,
                    width: pos.width.min(screen_area.width.saturating_sub(pos.x)),
                    height: pos.height.min(screen_area.height.saturating_sub(pos.y)),
                };

                // Skip if area is too small
                if area.width < 1 || area.height < 1 {
                    continue;
                }

                match &window.content {
                    WindowContent::Text(_) => {
                        // Use the TextWindow widget for proper text rendering with wrapping, scrolling, etc.
                        if let Some(text_window) = text_windows.get_mut(name) {
                            // Render with selection highlighting if active
                            let focused = app_core.ui_state.focused_window.as_ref() == Some(name);
                            let window_index = window_index_map.get(name).copied().unwrap_or(0);
                            text_window.render_with_focus(
                                area,
                                f.buffer_mut(),
                                focused,
                                app_core.ui_state.selection_state.as_ref(),
                                "#4a4a4a", // Selection background color
                                window_index,
                                &theme,
                            );
                        }
                    }
                    WindowContent::CommandInput { .. } => {
                        use crate::data::ui_state::InputMode;

                        // If in Search mode, render search input instead of command input
                        if app_core.ui_state.input_mode == InputMode::Search {
                            // Get search info from focused window (if any)
                            let search_info = if let Some(focused_name) =
                                &app_core.ui_state.focused_window
                            {
                                if let Some(window) = app_core.ui_state.windows.get(focused_name) {
                                    if let WindowContent::Text(_) = &window.content {
                                        text_windows
                                            .get(focused_name)
                                            .and_then(|tw| tw.search_info())
                                            .map(|(current, total)| {
                                                format!(" [{}/{}]", current + 1, total)
                                            })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                // No focused window, try main
                                if let Some(window) = app_core.ui_state.windows.get("main") {
                                    if let WindowContent::Text(_) = &window.content {
                                        text_windows
                                            .get("main")
                                            .and_then(|tw| tw.search_info())
                                            .map(|(current, total)| {
                                                format!(" [{}/{}]", current + 1, total)
                                            })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }
                            .unwrap_or_default();

                            // Build inline prompt with full help text (no borders)
                            let help_text = "(Enter:Search, Esc:Cancel, Ctrl+PgUp/PgDn:Navigate)";
                            let prompt = format!("Search{} {}: ", search_info, help_text);

                            // Split input into before/at/after cursor for proper cursor rendering
                            let input_text = &app_core.ui_state.search_input;
                            let cursor_pos = app_core.ui_state.search_cursor;
                            let chars: Vec<char> = input_text.chars().collect();
                            let before_cursor: String = chars.iter().take(cursor_pos).collect();
                            let cursor_char = chars.get(cursor_pos).copied().unwrap_or(' ');
                            let after_cursor: String = chars.iter().skip(cursor_pos + 1).collect();

                            let search_line = Line::from(vec![
                                Span::styled(prompt, Style::default().fg(Color::Yellow)),
                                Span::raw(before_cursor),
                                Span::styled(
                                    cursor_char.to_string(),
                                    Style::default().bg(Color::White).fg(Color::Black),
                                ),
                                Span::raw(after_cursor),
                            ]);

                            // Render directly without Block wrapper - no borders
                            let search_paragraph = Paragraph::new(search_line);
                            f.render_widget(search_paragraph, area);
                        } else {
                            // Normal mode - render command input
                            if let Some(cmd_input) = command_inputs.get(name) {
                                cmd_input.render(area, f.buffer_mut());
                            } else {
                                tracing::error!(
                                    "CommandInput widget '{}' doesn't exist during render!",
                                    name
                                );
                                // Render error message
                                let block = Block::default()
                                    .title("Command (ERROR: widget not initialized)")
                                    .borders(Borders::ALL);
                                f.render_widget(block, area);
                            }
                        }
                    }
                    WindowContent::Progress(_) => {
                        // Use the ProgressBar widget for proper rendering
                        if let Some(progress_bar) = progress_bars.get_mut(name) {
                            progress_bar.render_themed(area, f.buffer_mut(), &theme);
                        }
                    }
                    WindowContent::Countdown(_) => {
                        // Use the Countdown widget for proper rendering
                        if let Some(countdown_widget) = countdowns.get_mut(name) {
                            countdown_widget.render(
                                area,
                                f.buffer_mut(),
                                app_core.message_processor.server_time_offset,
                                &theme,
                            );
                        }
                    }
                    WindowContent::Indicator(_) => {
                        // Use the Indicator widget for proper rendering
                        if let Some(indicator_widget) = indicator_widgets.get_mut(name) {
                            indicator_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::ActiveEffects(_effects_content) => {
                        // Use the ActiveEffects widget for proper rendering
                        if let Some(active_effects_widget) = active_effects_windows.get_mut(name) {
                            active_effects_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::Hand { .. } => {
                        // Use the Hand widget for proper component-based rendering
                        if let Some(hand_widget) = hand_widgets.get_mut(name) {
                            hand_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::Room(_) => {
                        // Use the RoomWindow widget for proper component-based rendering
                        if let Some(room_window) = room_windows.get_mut(name) {
                            room_window.render_themed(area, f.buffer_mut(), &theme);
                        }
                    }
                    WindowContent::Inventory(_) => {
                        // Use the InventoryWindow widget for proper link rendering
                        if let Some(inventory_window) = inventory_windows.get_mut(name) {
                            inventory_window.render_themed(area, f.buffer_mut(), &theme);
                        }
                    }
                    WindowContent::Spells(_) => {
                        // Use the SpellsWindow widget for proper link rendering
                        if let Some(spells_window) = spells_windows.get_mut(name) {
                            spells_window.render_themed(area, f.buffer_mut(), &theme);
                        }
                    }
                    WindowContent::Targets { .. } => {
                        // Use the Targets widget
                        if let Some(targets_widget) = targets_widgets.get_mut(name) {
                            targets_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::Players { .. } => {
                        // Use the Players widget
                        if let Some(players_widget) = players_widgets.get_mut(name) {
                            players_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::Dashboard { .. } => {
                        // Use the Dashboard widget
                        if let Some(dashboard_widget) = dashboard_widgets.get_mut(name) {
                            dashboard_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::TabbedText(_) => {
                        // Use the TabbedTextWindow widget
                        if let Some(tabbed_window) = tabbed_text_windows.get_mut(name) {
                            let focused =
                                app_core.ui_state.focused_window.as_ref() == Some(name);
                            let window_index =
                                window_index_map.get(name).copied().unwrap_or(0);
                            tabbed_window.render_with_focus(
                                area,
                                f.buffer_mut(),
                                focused,
                                app_core.ui_state.selection_state.as_ref(),
                                "#4a4a4a", // Selection background color
                                window_index,
                                &theme,
                            );
                        }
                    }
                    WindowContent::Compass(_) => {
                        // Use the Compass widget
                        if let Some(compass_widget) = compass_widgets.get_mut(name) {
                            compass_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::InjuryDoll(_) => {
                        // Use the InjuryDoll widget
                        if let Some(injury_doll_widget) = injury_doll_widgets.get_mut(name) {
                            injury_doll_widget.render(area, f.buffer_mut());
                        }
                    }
                    WindowContent::Empty => {
                        // Check if this is a spacer widget
                        if window.widget_type == crate::data::WidgetType::Spacer {
                            if let Some(spacer_widget) = spacer_widgets.get_mut(name) {
                                spacer_widget.render(area, f.buffer_mut());
                            }
                        }
                        // Otherwise render nothing (empty placeholder)
                    }
                    WindowContent::Performance => {
                        if let Some(perf_widget) = performance_widgets.get_mut(name) {
                            perf_widget.render(area, f.buffer_mut(), &app_core.perf_stats);
                        }
                    }
                    WindowContent::Perception(_) => {
                        if let Some(perception_window) = perception_windows.get_mut(name) {
                            perception_window.render(area, f.buffer_mut());
                        }
                    }
                }
            }

            // Render popup menu if active
            if let Some(ref popup_menu) = app_core.ui_state.popup_menu {
                // Convert from ui_state::PopupMenu to rendering popup_menu::PopupMenu
                // Filter out disabled items
                let menu_items: Vec<popup_menu::MenuItem> = popup_menu
                    .items
                    .iter()
                    .filter(|item| !item.disabled)
                    .map(|item| popup_menu::MenuItem {
                        text: item.text.clone(),
                        command: item.command.clone(),
                    })
                    .collect();

                let render_menu = popup_menu::PopupMenu::with_selected(
                    menu_items,
                    popup_menu.position,
                    popup_menu.selected,
                );
                render_menu.render(screen_area, f.buffer_mut(), &theme);
            }

            // Render submenu if active (level 2)
            if let Some(ref submenu) = app_core.ui_state.submenu {
                // Filter out disabled items
                let menu_items: Vec<popup_menu::MenuItem> = submenu
                    .items
                    .iter()
                    .filter(|item| !item.disabled)
                    .map(|item| popup_menu::MenuItem {
                        text: item.text.clone(),
                        command: item.command.clone(),
                    })
                    .collect();

                let render_submenu = popup_menu::PopupMenu::with_selected(
                    menu_items,
                    submenu.position,
                    submenu.selected,
                );
                render_submenu.render(screen_area, f.buffer_mut(), &theme);
            }

            // Render nested submenu if active (level 3)
            if let Some(ref nested_submenu) = app_core.ui_state.nested_submenu {
                // Filter out disabled items
                let menu_items: Vec<popup_menu::MenuItem> = nested_submenu
                    .items
                    .iter()
                    .filter(|item| !item.disabled)
                    .map(|item| popup_menu::MenuItem {
                        text: item.text.clone(),
                        command: item.command.clone(),
                    })
                    .collect();

                let render_nested = popup_menu::PopupMenu::with_selected(
                    menu_items,
                    nested_submenu.position,
                    nested_submenu.selected,
                );
                render_nested.render(screen_area, f.buffer_mut(), &theme);
            }

            // Render deep submenu if active (level 4)
            if let Some(ref deep_submenu) = app_core.ui_state.deep_submenu {
                // Filter out disabled items
                let menu_items: Vec<popup_menu::MenuItem> = deep_submenu
                    .items
                    .iter()
                    .filter(|item| !item.disabled)
                    .map(|item| popup_menu::MenuItem {
                        text: item.text.clone(),
                        command: item.command.clone(),
                    })
                    .collect();

                let render_deep = popup_menu::PopupMenu::with_selected(
                    menu_items,
                    deep_submenu.position,
                    deep_submenu.selected,
                );
                render_deep.render(screen_area, f.buffer_mut(), &theme);
            }

            // Render browsers and forms if active
            if let Some(ref mut highlight_browser) = self.highlight_browser {
                highlight_browser.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut highlight_form) = self.highlight_form {
                highlight_form.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut keybind_browser) = self.keybind_browser {
                keybind_browser.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut keybind_form) = self.keybind_form {
                keybind_form.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut color_palette_browser) = self.color_palette_browser {
                color_palette_browser.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut color_form) = self.color_form {
                color_form.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut uicolors_browser) = self.uicolors_browser {
                uicolors_browser.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut spell_color_browser) = self.spell_color_browser {
                spell_color_browser.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut spell_color_form) = self.spell_color_form {
                spell_color_form.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref mut theme_editor) = self.theme_editor {
                theme_editor.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }
            if let Some(ref theme_browser) = self.theme_browser {
                
                f.render_widget(theme_browser, screen_area);
            }
            if let Some(ref mut settings_editor) = self.settings_editor {
                settings_editor.render(screen_area, f.buffer_mut(), &app_core.config, &theme);
            }

            if let Some(ref mut indicator_template_editor) = self.indicator_template_editor {
                indicator_template_editor.render(screen_area, f.buffer_mut(), &theme);
            }

            // Render window editor if active
            if let Some(ref mut window_editor) = self.window_editor {
                // Window editor handles its own positioning and sizing (70x20)
                let editor_theme = theme.to_editor_theme();
                window_editor.render(screen_area, f.buffer_mut(), &editor_theme);
            }
        })?;

        // Feed text wrapping timings into performance stats (drain samples from all text widgets)
        for text_window in text_windows.values_mut() {
            for sample in text_window.take_wrap_samples() {
                app_core.perf_stats.record_text_wrap_time(sample);
            }
        }
        for tabbed_window in tabbed_text_windows.values_mut() {
            for sample in tabbed_window.take_wrap_samples() {
                app_core.perf_stats.record_text_wrap_time(sample);
            }
        }

        // Record basic frame/render timings for the performance overlay
        let render_duration = render_start.elapsed();
        app_core.perf_stats.record_render_time(render_duration);
        app_core.perf_stats.record_ui_render_time(render_duration);
        app_core.perf_stats.record_frame();

        // Lightweight memory snapshot: number of tracked windows (keeps totals non-zero)
        let total_windows = app_core.ui_state.windows.len();
        let total_lines: usize = text_windows
            .values()
            .map(|tw| tw.wrapped_line_count())
            .sum::<usize>()
            + tabbed_text_windows
                .values()
                .map(|tw| tw.total_wrapped_line_count())
                .sum::<usize>();
        app_core
            .perf_stats
            .update_memory_stats(total_lines, total_windows);

        // Restore widgets
        self.widget_manager.text_windows = text_windows;
        self.widget_manager.command_inputs = command_inputs;
        self.widget_manager.room_windows = room_windows;
        self.widget_manager.inventory_windows = inventory_windows;
        self.widget_manager.spells_windows = spells_windows;
        self.widget_manager.perception_windows = perception_windows;
        self.widget_manager.progress_bars = progress_bars;
        self.widget_manager.countdowns = countdowns;
        self.widget_manager.active_effects_windows = active_effects_windows;
        self.widget_manager.hand_widgets = hand_widgets;
        self.widget_manager.spacer_widgets = spacer_widgets;
        self.widget_manager.indicator_widgets = indicator_widgets;
        self.widget_manager.targets_widgets = targets_widgets;
        self.widget_manager.players_widgets = players_widgets;
        self.widget_manager.dashboard_widgets = dashboard_widgets;
        self.widget_manager.tabbed_text_windows = tabbed_text_windows;
        self.widget_manager.compass_widgets = compass_widgets;
        self.widget_manager.injury_doll_widgets = injury_doll_widgets;
        self.widget_manager.performance_widgets = performance_widgets;

        Ok(())
    }

    fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        )?;
        Ok(())
    }

    fn size(&self) -> (u16, u16) {
        let rect = self.terminal.size().unwrap_or_default();
        (rect.width, rect.height)
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
