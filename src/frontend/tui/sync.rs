use super::*;
use super::performance_stats;
use super::colors::{blend_colors_hex, color_to_hex_string, normalize_color, parse_hex_color};
use std::char;

fn decode_icon(icon_str: &str) -> Option<String> {
    let trimmed = icon_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    let hex = trimmed
        .trim_start_matches("0x")
        .trim_start_matches("\\u{")
        .trim_end_matches('}');
    if hex.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(codepoint) = u32::from_str_radix(hex, 16) {
            if let Some(ch) = char::from_u32(codepoint) {
                return Some(ch.to_string());
            }
        }
    }

    // Fallback: use the first character as-is
    trimmed.chars().next().map(|c| c.to_string())
}

impl TuiFrontend {
    pub(crate) fn sync_text_windows(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Text(text_content) = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create TextWindow for this window
                let text_window = self.widget_manager.text_windows.entry(name.clone()).or_insert_with(|| {
                    let mut tw =
                        text_window::TextWindow::new(&text_content.title, text_content.max_lines);

                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        tw.set_border_config(
                            def.base().show_border,
                            Some(def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        tw.set_border_sides(def.base().border_sides.clone());
                        tw.set_background_color(colors.background.clone());
                        tw.set_text_color(colors.text.clone());
                        tw.set_content_align(def.base().content_align.clone());
                        tw.set_title_position(super::title_position::TitlePosition::from_str(
                            &def.base().title_position,
                        ));
                        if let crate::config::WindowDef::Text { data, .. } = def {
                            tw.set_show_timestamps(data.show_timestamps);
                            let ts_pos = data.timestamp_position
                                .unwrap_or(app_core.config.ui.timestamp_position);
                            tw.set_timestamp_position(ts_pos);
                            tw.set_wordwrap(data.wordwrap);
                            // Compact mode automatically centers content
                            if data.compact {
                                tw.set_content_align(Some("center".to_string()));
                            }
                        } else {
                            tw.set_show_timestamps(false); // Default to false for non-text windows
                            tw.set_timestamp_position(app_core.config.ui.timestamp_position);
                            tw.set_wordwrap(true);
                        }
                    } else {
                        tw.set_show_timestamps(false); // Default to false when no window def
                        tw.set_timestamp_position(crate::config::TimestampPosition::End);
                        tw.set_wordwrap(true);
                    }
                    // Note: Highlights are now applied in core (MessageProcessor)

                    tw
                });

                // Existing text windows need to reapply theme-derived settings when themes change
                if let Some(def) = window_def {
                    let colors = resolve_window_colors(def.base(), theme);
                    text_window.set_border_config(
                        def.base().show_border,
                        Some(def.base().border_style.clone()),
                        colors.border.clone(),
                    );
                    text_window.set_border_sides(def.base().border_sides.clone());
                    text_window.set_background_color(colors.background.clone());
                    text_window.set_text_color(colors.text.clone());
                    text_window.set_content_align(def.base().content_align.clone());
                    let title_text = if def.base().show_title {
                        def.base().title.clone().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    text_window.set_title(title_text);
                    text_window.set_title_position(super::title_position::TitlePosition::from_str(
                        &def.base().title_position,
                    ));
                    if let crate::config::WindowDef::Text { data, .. } = def {
                        text_window.set_show_timestamps(data.show_timestamps);
                        let ts_pos = data.timestamp_position
                            .unwrap_or(app_core.config.ui.timestamp_position);
                        text_window.set_timestamp_position(ts_pos);
                        text_window.set_wordwrap(data.wordwrap);
                        // Compact mode automatically centers content
                        if data.compact {
                            text_window.set_content_align(Some("center".to_string()));
                        }
                    } else {
                        text_window.set_show_timestamps(false); // Default to false
                        text_window.set_timestamp_position(app_core.config.ui.timestamp_position);
                        text_window.set_wordwrap(true);
                    }
                }

                // Update width for proper wrapping
                text_window.set_width(window.position.width);

                // Get last synced generation
                let last_synced_gen = self.widget_manager.last_synced_generation.get(name).copied().unwrap_or(0);
                let current_gen = text_content.generation;

                // Check if there are new lines to sync (generation changed)
                if current_gen > last_synced_gen {
                    // Calculate how many lines to add
                    // If generation delta > line count, we need to resync entire buffer
                    let gen_delta = (current_gen - last_synced_gen) as usize;
                    let needs_full_resync = gen_delta > text_content.lines.len();

                    if needs_full_resync {
                        // Full resync - clear and add all lines
                        tracing::trace!(
                            "Text window '{}': full resync (gen delta {} > line count {})",
                            name,
                            gen_delta,
                            text_content.lines.len()
                        );
                        text_window.clear();
                    }

                    // Determine how many lines to add
                    let lines_to_add = if needs_full_resync {
                        text_content.lines.len() // Add all lines
                    } else {
                        gen_delta.min(text_content.lines.len()) // Add only new lines
                    };

                    let skip_count = text_content.lines.len().saturating_sub(lines_to_add);
                    for line in text_content.lines.iter().skip(skip_count) {
                        // Set the stream for this line so stream-filtered highlights work
                        text_window.set_current_stream(&line.stream);

                        // Convert our data format to TextWindow's format
                        for segment in &line.segments {
                            // TextWindow now uses the same SpanType and LinkData from data module
                            let styled_text = text_window::StyledText {
                                content: segment.text.clone(),
                                fg: segment
                                    .fg
                                    .as_ref()
                                    .and_then(|hex| parse_hex_color(hex).ok()),
                                bg: segment
                                    .bg
                                    .as_ref()
                                    .and_then(|hex| parse_hex_color(hex).ok()),
                                bold: segment.bold,
                                span_type: segment.span_type,  // Direct use, no conversion needed
                                link_data: segment.link_data.clone(),  // Direct use, no conversion needed
                            };
                            text_window.add_text(styled_text);
                        }
                        // Finish the line with actual window width
                        text_window.finish_line(window.position.width);
                    }

                    // Update last synced generation
                    self.widget_manager.last_synced_generation
                        .insert(name.clone(), current_gen);
                }

                // Sync scroll offset from data layer to TextWindow
                // TextContent scroll_offset is lines from bottom (0 = live view)
                // TextWindow scroll methods handle this the same way
                // Note: TextWindow doesn't have a direct set_scroll_offset, so we'd need to
                // track the last known offset and call scroll_up/scroll_down as needed
                // For now, this is handled by user input events that modify both layers
            } else if let crate::data::WindowContent::Room(_room_content) = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create RoomWindow for this window
                if !self.widget_manager.room_windows.contains_key(name) {
                    let mut room_window = room_window::RoomWindow::new("Room".to_string());

                    // Configure RoomWindow with settings from WindowDef
                    if let Some(crate::config::WindowDef::Room { data, .. }) = window_def {
                        // Set component visibility from config
                        room_window.set_component_visible("room desc", data.show_desc);
                        room_window.set_component_visible("room objs", data.show_objs);
                        room_window.set_component_visible("room players", data.show_players);
                        room_window.set_component_visible("room exits", data.show_exits);
                    }

                    // Apply roomName preset colors to the title/room name if available
                    // Resolve palette names to hex values
                    if let Some(preset) = app_core.config.colors.presets.get("roomName") {
                        let resolved_fg = preset.fg.as_ref().map(|c| app_core.config.resolve_palette_color(c));
                        let resolved_bg = preset.bg.as_ref().map(|c| app_core.config.resolve_palette_color(c));
                        room_window.set_title_colors(resolved_fg, resolved_bg);
                    }

                    self.widget_manager.room_windows.insert(name.clone(), room_window);
                    tracing::debug!("Created RoomWindow widget for '{}' during sync", name);
                }
            }
            // TODO: Add similar widget creation for other complex widget types as they're implemented:
            // - Progress bars (if they need stateful widgets beyond simple rendering)
            // - Countdown timers (if they need stateful widgets)
            // - Compass (if it needs stateful widgets)
            // - Indicator (if it needs stateful widgets)
            // - Hands/Inventory (if they need stateful widgets)
            // - Dashboard (if it needs stateful widgets)
            // Currently these render directly in the render loop without needing persistent widget state,
            // but if they gain more complex behavior (animations, interactions, etc.), they'll need
            // to be created here during sync just like Room and Text windows.
        }
    }

    /// Sync command input widgets with window configuration
    pub(crate) fn sync_command_inputs(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if !matches!(
                window.content,
                crate::data::WindowContent::CommandInput { .. }
            ) {
                continue;
            }

            let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);
            let (base_config, cmd_data) = match window_def {
                Some(crate::config::WindowDef::CommandInput { base, data }) => {
                    (Some(base.clone()), Some(data.clone()))
                }
                Some(def) => (Some(def.base().clone()), None),
                None => (None, None),
            };

            // Ensure the backing widget exists so we can apply configuration
            let cmd_input = self.widget_manager.command_inputs.entry(name.clone()).or_insert_with(|| {
                let mut widget = command_input::CommandInput::new(1000);
                if let Some(base) = base_config.as_ref() {
                    let title_text = if base.show_title {
                        base.title.clone().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    widget.set_title(title_text);
                    widget.set_show_title(base.show_title);
                } else {
                    widget.set_title("Command".to_string());
                }
                widget
            });

            if let Some(base) = base_config.as_ref() {
                let title_text = if base.show_title {
                    base.title.clone().unwrap_or_default()
                } else {
                    String::new()
                };
                cmd_input.set_title(title_text);
                cmd_input.set_title_position(super::title_position::TitlePosition::from_str(
                    &base.title_position,
                ));
                cmd_input.set_show_title(base.show_title);
                let border_color = normalize_color(&base.border_color)
                    .or_else(|| color_to_hex_string(&theme.window_border));
                cmd_input.set_border_config(
                    base.show_border,
                    Some(base.border_style.clone()),
                    border_color,
                );
                cmd_input.set_border_sides(base.border_sides.clone());
                cmd_input.set_show_title(base.show_title);
                let background_color = if base.transparent_background {
                    None
                } else {
                    normalize_color(&base.background_color)
                        .or_else(|| color_to_hex_string(&theme.window_background))
                };
                cmd_input.set_background_color(background_color);
                let text_color = cmd_data
                    .as_ref()
                    .and_then(|d| normalize_color(&d.text_color))
                    .or_else(|| normalize_color(&base.text_color))
                    .or_else(|| color_to_hex_string(&theme.text_primary));
                cmd_input.set_text_color(text_color);
                let cursor_fg = cmd_data
                    .as_ref()
                    .and_then(|d| normalize_color(&d.cursor_color))
                    .or_else(|| color_to_hex_string(&theme.window_background));
                let cursor_bg = cmd_data
                    .as_ref()
                    .and_then(|d| normalize_color(&d.cursor_background_color))
                    .or_else(|| color_to_hex_string(&theme.text_primary));
                cmd_input.set_cursor_colors(cursor_fg, cursor_bg);
                let prompt_icon = cmd_data
                    .as_ref()
                    .and_then(|d| d.prompt_icon.clone())
                    .filter(|s| !s.trim().is_empty());
                cmd_input.set_prompt_icon(prompt_icon);
                let prompt_icon_color = cmd_data
                    .as_ref()
                    .and_then(|d| normalize_color(&d.prompt_icon_color))
                    .or_else(|| normalize_color(&base.text_color))
                    .or_else(|| color_to_hex_string(&theme.text_primary));
                cmd_input.set_prompt_icon_color(prompt_icon_color);
            }
        }
    }

    /// Sync inventory window data - create/configure widgets
    pub(crate) fn sync_inventory_windows(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find inventory windows in ui_state
        for (name, window) in &app_core.ui_state.windows {
            // Check for both Inventory and Text content types
            let text_content = match &window.content {
                crate::data::WindowContent::Inventory(content) => Some(content),
                crate::data::WindowContent::Text(content)
                    if name == "inventory"
                        || content.title.to_lowercase().contains("inventory") =>
                {
                    Some(content)
                }
                _ => None,
            };

            if let Some(text_content) = text_content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create InventoryWindow for this window
                if !self.widget_manager.inventory_windows.contains_key(name) {
                    let mut inv_window =
                        inventory_window::InventoryWindow::new(text_content.title.clone());
                    // GameState widgets still need highlight patterns for item highlighting
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    inv_window.set_highlights(highlights);
                    inv_window.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                    self.widget_manager.inventory_windows.insert(name.clone(), inv_window);
                    tracing::debug!("Created InventoryWindow widget for '{}'", name);
                }

                // Update configuration and content from WindowDef if present
                if let Some(inv_window) = self.widget_manager.inventory_windows.get_mut(name) {
                    inv_window.set_title(text_content.title.clone());
                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        inv_window.set_border_config(def.base().show_border, colors.border.clone());
                        inv_window.set_transparent_background(def.base().transparent_background);
                        inv_window.set_background_color(colors.background.clone());
                        inv_window.set_text_color(colors.text.clone());
                        let title_text = if def.base().show_title {
                            def.base()
                                .title
                                .clone()
                                .unwrap_or_else(|| text_content.title.clone())
                        } else {
                            String::new()
                        };
                        inv_window.set_title(title_text);
                    }

                    // Change detection: only sync if content changed (using generation)
                    let last_synced_gen =
                        self.widget_manager.last_synced_generation.get(name).copied().unwrap_or(0);
                    let current_gen = text_content.generation;

                    if current_gen != last_synced_gen {
                        // Content changed - sync text lines from WindowContent to widget
                        inv_window.clear();
                        tracing::debug!("Syncing inventory widget '{}' with {} lines (gen changed from {} to {})",
                            name, text_content.lines.len(), last_synced_gen, current_gen);
                        for line in &text_content.lines {
                            for segment in &line.segments {
                                inv_window.add_segment(segment.clone());
                            }
                            inv_window.finish_line();
                        }
                        // Update last synced generation
                        self.widget_manager.last_synced_generation
                            .insert(name.clone(), current_gen);
                    }
                } else {
                    tracing::warn!(
                        "Inventory widget '{}' not found in inventory_windows HashMap!",
                        name
                    );
                }
            }
        }
    }

    /// Sync spells window data - create/configure widgets
    pub(crate) fn sync_spells_windows(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find spells windows in ui_state
        for (name, window) in &app_core.ui_state.windows {
            // Check for Spells content type
            let text_content = match &window.content {
                crate::data::WindowContent::Spells(content) => Some(content),
                _ => None,
            };

            if let Some(text_content) = text_content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create SpellsWindow for this window
                if !self.widget_manager.spells_windows.contains_key(name) {
                    let mut spells_window =
                        spells_window::SpellsWindow::new(text_content.title.clone());
                    // GameState widgets still need highlight patterns for item highlighting
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    spells_window.set_highlights(highlights);
                    spells_window.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                    self.widget_manager.spells_windows.insert(name.clone(), spells_window);
                    tracing::debug!("Created SpellsWindow widget for '{}'", name);
                }

                // Update configuration and content from WindowDef if present
                if let Some(spells_window) = self.widget_manager.spells_windows.get_mut(name) {
                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        spells_window.set_border_config(
                            def.base().show_border,
                            Some(def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        spells_window.set_transparent_background(def.base().transparent_background);
                        spells_window.set_background_color(colors.background.clone());
                        spells_window.set_text_color(colors.text.clone());
                        let title_text = if def.base().show_title {
                            def.base()
                                .title
                                .clone()
                                .unwrap_or_else(|| text_content.title.clone())
                        } else {
                            String::new()
                        };
                        spells_window.set_title(title_text);
                    } else {
                        spells_window.set_title(text_content.title.clone());
                    }

                    // Change detection: only sync if content changed (using generation)
                    let last_synced_gen =
                        self.widget_manager.last_synced_generation.get(name).copied().unwrap_or(0);
                    let current_gen = text_content.generation;

                    if current_gen != last_synced_gen {
                        // Content changed - sync text lines from WindowContent to widget
                        spells_window.clear();
                        tracing::debug!(
                            "Syncing spells widget '{}' with {} lines (gen changed from {} to {})",
                            name,
                            text_content.lines.len(),
                            last_synced_gen,
                            current_gen
                        );
                        for line in &text_content.lines {
                            for segment in &line.segments {
                                spells_window.add_text(
                                    segment.text.clone(),
                                    segment.fg.clone(),
                                    segment.bg.clone(),
                                    segment.bold,
                                    segment.span_type,
                                    segment.link_data.clone(),
                                );
                            }
                            spells_window.finish_line();
                        }
                        // Update last synced generation
                        self.widget_manager.last_synced_generation
                            .insert(name.clone(), current_gen);
                    }
                } else {
                    tracing::warn!(
                        "Spells widget '{}' not found in spells_windows HashMap!",
                        name
                    );
                }
            }
        }
    }

    /// Sync progress bar data - create/configure widgets
    pub(crate) fn sync_progress_bars(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find progress bar windows in ui_state
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Progress(progress_data) = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create ProgressBar for this window
                if !self.widget_manager.progress_bars.contains_key(name) {
                    let label = window_def
                        .and_then(|def| {
                            if def.base().show_title {
                                def.base().title.as_ref()
                            } else {
                                None
                            }
                        })
                        .cloned()
                        .unwrap_or_default();

                    let bar = progress_bar::ProgressBar::new(&label);
                    self.widget_manager.progress_bars.insert(name.clone(), bar);
                    tracing::debug!("Created ProgressBar widget for '{}'", name);
                }

                // Update configuration and value
                if let Some(progress_bar) = self.widget_manager.progress_bars.get_mut(name) {
                    // Set value from game data
                    if let Some(ref custom_text) = progress_data.color {
                        // color field is being used as custom text (e.g., "clear as a bell")
                        progress_bar.set_value_with_text(
                            progress_data.value,
                            progress_data.max,
                            Some(custom_text.clone()),
                        );
                    } else {
                        progress_bar.set_value(progress_data.value, progress_data.max);
                    }

                    // Apply window config from WindowDef
                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        progress_bar.set_border_config(
                            def.base().show_border,
                            Some(def.base().border_style.clone()),
                            colors.border.clone(),
                            def.base().border_sides.clone(),
                        );

                        // Update title visibility
                        if def.base().show_title {
                            progress_bar.set_title(def.base().title.clone().unwrap_or_default());
                        } else {
                            progress_bar.set_title(String::new());
                        }

                        // Compute display text based on config and incoming text
                        let display_text = if let crate::config::WindowDef::Progress { data, .. } = def {
                            let (label, current_val, max_val) = Self::parse_progress_display_parts(
                                &progress_data.label,
                                progress_data.value,
                                progress_data.max,
                            );

                            if data.current_only {
                                format!("{}", current_val)
                            } else if data.numbers_only {
                                format!("{}/{}", current_val, max_val)
                            } else if !progress_data.label.trim().is_empty() {
                                progress_data.label.clone()
                            } else if let Some(lbl) = label {
                                format!("{} {}/{}", lbl, current_val, max_val)
                            } else {
                                format!("{}/{}", current_val, max_val)
                            }
                        } else {
                            format!("{}/{}", progress_data.value, progress_data.max)
                        };
                        progress_bar.set_value_with_text(
                            progress_data.value,
                            progress_data.max,
                            Some(display_text),
                        );

                        // Get bar color from ProgressWidgetData, or fallback to VellumFE defaults
                        if let crate::config::WindowDef::Progress { data, .. } = def {
                            let bar_color = if let Some(ref color) = data.color {
                                Some(color.clone())
                            } else {
                                // Fallback to VellumFE template colors for known progress bars
                                match name.as_str() {
                                    "health" => Some("#6e0202".to_string()),     // Dark red
                                    "mana" => Some("#08086d".to_string()),       // Dark blue
                                    "stamina" => Some("#bd7b00".to_string()),    // Orange
                                    "spirit" => Some("#6e727c".to_string()),     // Gray
                                    "encumlevel" => Some("#ffff00".to_string()), // Yellow
                                    "pbarStance" => Some("#ffa500".to_string()), // Orange
                                    "mindState" => Some("#9370db".to_string()),  // Purple
                                    "lblBPs" => Some("#ff4500".to_string()),     // Orange-red
                                    _ => None,
                                }
                            };

                            if let Some(color) = bar_color {
                                progress_bar.set_colors(Some(color), None);
                            }
                        }

                        // Apply text color
                        progress_bar.set_text_color(colors.text.clone());

                        // Apply transparent/background handling
                        let bg = colors
                            .background
                            .clone()
                            .or_else(|| color_to_hex_string(&theme.window_background));
                        progress_bar.set_transparent_background(def.base().transparent_background);
                        progress_bar.set_background_color(bg);
                    }
                }
            }
        }
    }

    /// Sync countdown data - create/configure countdown widgets
    pub(crate) fn sync_countdowns(&mut self, app_core: &crate::core::AppCore, theme: &crate::theme::AppTheme) {
        // Find countdown windows in ui_state
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Countdown(countdown_data) = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create Countdown for this window
                if !self.widget_manager.countdowns.contains_key(name) {
                    let label = window_def
                        .and_then(|def| {
                            if def.base().show_title {
                                def.base().title.as_ref()
                            } else {
                                None
                            }
                        })
                        .cloned()
                        .unwrap_or_default();

                    let countdown = countdown::Countdown::new(&label);
                    self.widget_manager.countdowns.insert(name.clone(), countdown);
                    tracing::debug!("Created Countdown widget for '{}'", name);
                }

                // Update configuration and value
                if let Some(countdown_widget) = self.widget_manager.countdowns.get_mut(name) {
                    // Set end time from game data
                    countdown_widget.set_end_time(countdown_data.end_time);

                    // Apply window config from WindowDef
                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        countdown_widget.set_border_config(
                            def.base().show_border,
                            Some(def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        countdown_widget.set_border_sides(def.base().border_sides.clone());
                        let title_text = if def.base().show_title {
                            def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        countdown_widget.set_title(title_text);
                        countdown_widget
                            .set_title_position(def.base().title_position.clone());

                        // Get icon from CountdownWidgetData
                        if let crate::config::WindowDef::Countdown { data, .. } = def {
                            if let Some(icon) = data.icon {
                                countdown_widget.set_icon(icon);
                            }
                            let text_color = data.color.clone().or_else(|| colors.text.clone());
                            countdown_widget.set_text_color(text_color);
                            let bg_color = data
                                .background_color
                                .clone()
                                .or_else(|| def.base().background_color.clone())
                                .or_else(|| color_to_hex_string(&theme.window_background));
                            countdown_widget.set_background_color(bg_color);
                        } else {
                            countdown_widget.set_text_color(colors.text.clone());
                        }

                        countdown_widget
                            .set_transparent_background(def.base().transparent_background);
                    }
                }
            }
        }
    }

    /// Sync active effects data - create/configure active effects widgets
    pub(crate) fn sync_active_effects(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find active effects windows in ui_state
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::ActiveEffects(effects_content) = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create ActiveEffects for this window
                if !self.widget_manager.active_effects_windows.contains_key(name) {
                    let label = window_def
                        .and_then(|def| {
                            if def.base().show_title {
                                def.base().title.as_ref()
                            } else {
                                None
                            }
                        })
                        .cloned()
                        .unwrap_or_default();

                    let mut widget = active_effects::ActiveEffects::new(
                        &label,
                        effects_content.category.clone(),
                    );
                    // GameState widgets still need highlight patterns for item highlighting
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    widget.set_highlights(highlights);
                    widget.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                    self.widget_manager.active_effects_windows.insert(name.clone(), widget);
                    tracing::debug!("Created ActiveEffects widget for '{}'", name);
                }

                // Update effects data and configuration
                if let Some(widget) = self.widget_manager.active_effects_windows.get_mut(name) {
                    let previous_scroll = widget.scroll_position();

                    // Clear existing effects
                    widget.clear();

                    // Add all effects from content
                    for effect in &effects_content.effects {
                        widget.add_or_update_effect(
                            effect.id.clone(),
                            effect.text.clone(),
                            effect.value,
                            effect.time.clone(),
                            effect.bar_color.clone(),
                            effect.text_color.clone(),
                        );
                    }

                    widget.restore_scroll_position(previous_scroll);

                    // Apply window config from WindowDef
                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        widget.set_border_config(
                            def.base().show_border,
                            Some(def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        widget.set_border_sides(def.base().border_sides.clone());
                        let title_text = if def.base().show_title {
                            def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        widget.set_title(title_text);
                        widget.set_transparent_background(def.base().transparent_background);
                        widget.set_background_color(colors.background.clone());
                        widget.set_text_color(colors.text.clone());
                    }
                }
            }
        }
    }

    /// Sync spacer widget data from AppCore to spacer widgets
    pub(crate) fn sync_spacer_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find all Spacer windows in the UI state (Empty content + Spacer widget type)
        for (name, window) in &app_core.ui_state.windows {
            if window.widget_type == crate::data::WidgetType::Spacer {
                // Ensure spacer widget exists in cache
                if !self.widget_manager.spacer_widgets.contains_key(name) {
                    let widget = spacer::Spacer::new();
                    self.widget_manager.spacer_widgets.insert(name.clone(), widget);
                }

                // Update spacer widget configuration
                if let Some(spacer_widget) = self.widget_manager.spacer_widgets.get_mut(name) {
                    // Apply window configuration from layout
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        spacer_widget.set_background_color(colors.background.clone());
                        spacer_widget
                            .set_transparent_background(window_def.base().transparent_background);
                    }
                }
            }
        }
    }

    /// Sync quickbar widget data from AppCore to quickbar widgets
    pub(crate) fn sync_quickbar_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if window.widget_type != crate::data::WidgetType::Quickbar {
                continue;
            }

            if !self.widget_manager.quickbar_widgets.contains_key(name) {
                let widget = quickbar::Quickbar::new(name);
                self.widget_manager.quickbar_widgets.insert(name.clone(), widget);
            }

            if let Some(quickbar_widget) = self.widget_manager.quickbar_widgets.get_mut(name) {
                let window_def = app_core.layout.windows.iter().find(|w| w.name() == name);
                let active_id = app_core
                    .ui_state
                    .active_quickbar_id
                    .clone()
                    .or_else(|| app_core.ui_state.quickbar_order.first().cloned());
                let quickbar_data = active_id
                    .as_ref()
                    .and_then(|id| app_core.ui_state.quickbars.get(id));
                let entries = quickbar_data
                    .map(|data| data.entries.clone())
                    .unwrap_or_default();
                quickbar_widget.set_entries(entries);

                if let Some(def) = window_def {
                    let colors = resolve_window_colors(def.base(), theme);
                    quickbar_widget.set_border_config(
                        def.base().show_border,
                        Some(def.base().border_style.clone()),
                        colors.border.clone(),
                    );
                    quickbar_widget.set_border_sides(def.base().border_sides.clone());
                    quickbar_widget.set_background_color(colors.background.clone());
                    quickbar_widget.set_text_color(colors.text.clone());
                    quickbar_widget.set_transparent_background(def.base().transparent_background);

                    let title_text = if def.base().show_title {
                        let data_title = quickbar_data
                            .and_then(|data| data.title.clone())
                            .filter(|t| !t.trim().is_empty());
                        data_title
                            .or_else(|| def.base().title.clone())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    quickbar_widget.set_title(title_text);
                }

                quickbar_widget.set_selection_colors(
                    color_to_hex_string(&theme.text_selected),
                    color_to_hex_string(&theme.background_selected),
                );
            }
        }
    }

    /// Sync indicator widget data from AppCore to indicator widgets
    pub(crate) fn sync_indicator_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find all Indicator windows in the UI state
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Indicator(indicator_data) = &window.content {
                // Ensure indicator widget exists in cache
                if !self.widget_manager.indicator_widgets.contains_key(name) {
                    let widget = indicator::Indicator::new(name);
                    self.widget_manager.indicator_widgets.insert(name.clone(), widget);
                }

                // Update indicator widget content and configuration
                if let Some(indicator_widget) = self.widget_manager.indicator_widgets.get_mut(name) {
                    // Set active state based on indicator data
                    indicator_widget.set_active(indicator_data.active);

                    // Apply window configuration from layout
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        indicator_widget.set_border_config(
                            window_def.base().show_border,
                            Some(window_def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        indicator_widget.set_border_sides(window_def.base().border_sides.clone());
                        let title_text = if let crate::config::WindowDef::Indicator { data, .. } = window_def
                        {
                            // Prefer icon from data if provided; fall back to title or empty
                            if let Some(ref icon_str) = data.icon {
                                decode_icon(icon_str).unwrap_or_else(|| {
                                    if window_def.base().show_title {
                                        window_def.base().title.clone().unwrap_or_default()
                                    } else {
                                        String::new()
                                    }
                                })
                            } else if window_def.base().show_title {
                                window_def.base().title.clone().unwrap_or_default()
                            } else {
                                String::new()
                            }
                        } else if window_def.base().show_title {
                            window_def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        indicator_widget.set_title(title_text);
                        indicator_widget.set_background_color(colors.background.clone());
                        indicator_widget
                            .set_transparent_background(window_def.base().transparent_background);

                        // Set custom colors if provided
                        if let Some(ref color) = indicator_data.color {
                            indicator_widget.set_colors("#555555".to_string(), color.clone());
                        }
                    }
                }
            }
        }
    }

    /// Sync targets widget data from GameState.room_creatures
    /// Uses component-based parsing (room objs) for creature data
    pub(crate) fn sync_targets_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Targets = &window.content {
                // Ensure widget exists
                if !self.widget_manager.targets_widgets.contains_key(name) {
                    tracing::debug!(
                        "sync_targets_widgets: Creating new widget '{}'",
                        name
                    );
                    let mut widget = targets::Targets::new(name);
                    // GameState widgets still need highlight patterns for item highlighting
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    widget.set_highlights(highlights);
                    widget.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                    self.widget_manager.targets_widgets.insert(name.clone(), widget);
                }

                // Update widget from GameState.room_creatures
                if let Some(widget) = self.widget_manager.targets_widgets.get_mut(name) {
                    let creature_count = app_core.game_state.room_creatures.len();
                    tracing::trace!(
                        "sync_targets_widgets: Updating '{}' with {} creatures",
                        name,
                        creature_count
                    );

                    // Get widget width from window definition
                    let widget_width = app_core
                        .layout
                        .windows
                        .iter()
                        .find(|w| w.name() == name)
                        .map(|w| w.base().cols)
                        .unwrap_or(20); // Fallback width if not found

                    widget.update_from_state(
                        &app_core.game_state.room_creatures,
                        &app_core.game_state.target_list.current_target,
                        &app_core.config.target_list,
                        widget_width,
                    );

                    // Apply configuration
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        widget.set_border_config(
                            window_def.base().show_border,
                            Some(window_def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        widget.set_border_sides(window_def.base().border_sides.clone());
                        widget.set_background_color(colors.background.clone());
                        widget.set_border_color(colors.border.clone());

                        // Use monsterbold preset as default text color for creatures,
                        // unless user explicitly set text_color in window config.
                        // Filter out "-" which means "use default" in the config system.
                        let explicit_text_color = window_def
                            .base()
                            .text_color
                            .clone()
                            .filter(|c| c != "-" && !c.is_empty());
                        let monsterbold_preset = app_core
                            .config
                            .colors
                            .presets
                            .get("monsterbold");
                        // Resolve palette name to hex value for monsterbold preset
                        let monsterbold_fg = monsterbold_preset
                            .and_then(|p| p.fg.as_ref())
                            .map(|c| app_core.config.resolve_palette_color(c));

                        let creature_text_color = explicit_text_color.or(monsterbold_fg);
                        widget.set_text_color(creature_text_color.clone());

                        // Set indicator color for current target (►)
                        let indicator_color = app_core
                            .config
                            .colors
                            .presets
                            .get("target_indicator")
                            .and_then(|p| p.fg.clone());

                        widget.set_indicator_color(indicator_color);

                        // Set body part count display option from widget data
                        if let crate::config::WindowDef::Targets { data, .. } = window_def {
                            widget.set_show_body_part_count(data.show_body_part_count);
                        }

                        // Respect user's transparent_background setting from window config
                        widget.set_transparent_background(window_def.base().transparent_background);

                        let base_title = window_def
                            .base()
                            .title
                            .clone()
                            .unwrap_or_else(|| name.clone());
                        if window_def.base().show_title {
                            widget.set_title(&base_title);
                        } else {
                            widget.set_title("");
                        }
                    }
                }
            }
        }
    }

    /// Sync container window widget data from GameState.container_cache
    /// Looks up containers by title (case-insensitive), since container IDs change each session
    pub(crate) fn sync_container_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Resolve "links" preset color, falling back to theme.link_color
        let link_color = app_core
            .config
            .colors
            .presets
            .get("links")
            .and_then(|preset| preset.fg.as_ref())
            .map(|c| Some(app_core.config.resolve_palette_color(c)))
            .unwrap_or_else(|| color_to_hex_string(&theme.link_color));

        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Container { container_title } = &window.content {
                // Ensure widget exists - use title as the identifier since it's persistent
                if !self.widget_manager.container_widgets.contains_key(name) {
                    // Use the container title from cache if available, otherwise use the container_title from config
                    let display_title = app_core
                        .game_state
                        .container_cache
                        .find_by_title(container_title)
                        .map(|c| c.title.clone())
                        .unwrap_or_else(|| container_title.clone());
                    let mut widget = container_window::ContainerWindow::new(container_title.clone(), display_title);
                    // GameState widgets still need highlight patterns for item highlighting
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    widget.set_highlights(highlights);
                    widget.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                    // Set link color from "links" preset before first update
                    widget.set_link_color(link_color.clone());
                    self.widget_manager.container_widgets.insert(name.clone(), widget);
                }

                // Update widget from GameState.container_cache
                if let Some(widget) = self.widget_manager.container_widgets.get_mut(name) {
                    // Apply link color from "links" preset (must be set before update_from_cache for correct parsing)
                    widget.set_link_color(link_color.clone());

                    // Look up container by title (case-insensitive match)
                    if let Some(container_data) = app_core.game_state.container_cache.find_by_title(container_title) {
                        widget.update_from_cache(container_data);
                    }

                    // Apply configuration
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        widget.set_border_config(
                            window_def.base().show_border,
                            colors.border.clone(),
                        );
                        widget.set_background_color(colors.background.clone());
                        widget.set_text_color(colors.text.clone());
                        widget.set_transparent_background(window_def.base().transparent_background);

                        // Set title from window def or fall back to container title
                        if window_def.base().show_title {
                            if let Some(ref title) = window_def.base().title {
                                widget.set_title(title.clone());
                            }
                            // Otherwise keep the title from container cache
                        } else {
                            widget.set_title(String::new());
                        }
                    } else {
                        // Ephemeral container windows (from .showcontainers) - apply theme defaults
                        widget.set_border_config(
                            true,
                            color_to_hex_string(&theme.window_border),
                        );
                        widget.set_background_color(color_to_hex_string(&theme.window_background));
                        widget.set_text_color(color_to_hex_string(&theme.text_primary));
                    }
                }
            }
        }
    }

    /// Sync players widget data from AppCore to players widgets (component-based)
    pub(crate) fn sync_players_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if matches!(window.content, crate::data::WindowContent::Players) {
                // Ensure widget exists
                if !self.widget_manager.players_widgets.contains_key(name) {
                    let mut widget = players::Players::new(name);
                    // Apply highlight patterns for text highlighting
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    widget.set_highlights(highlights);
                    widget.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                    self.widget_manager.players_widgets.insert(name.clone(), widget);
                }

                // Update widget from GameState.room_players
                if let Some(widget) = self.widget_manager.players_widgets.get_mut(name) {
                    widget.update_from_state(
                        &app_core.game_state.room_players,
                        &app_core.config.target_list,
                    );

                    // Apply configuration (borders, colors, title)
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        widget.set_border_config(
                            window_def.base().show_border,
                            Some(window_def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        widget.set_border_sides(window_def.base().border_sides.clone());
                        widget.set_background_color(colors.background.clone());
                        widget.set_text_color(colors.text.clone());
                        widget.set_transparent_background(window_def.base().transparent_background);

                        let base_title = window_def
                            .base()
                            .title
                            .clone()
                            .unwrap_or_else(|| name.clone());
                        if window_def.base().show_title {
                            widget.set_title(&base_title);
                        } else {
                            widget.set_title("");
                        }
                    }
                }
            }
        }
    }

    /// Sync dashboard widget data from AppCore to dashboard widgets
    pub(crate) fn sync_dashboard_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Dashboard { indicators } = &window.content {
                // Ensure widget exists
                if !self.widget_manager.dashboard_widgets.contains_key(name) {
                    // Default to horizontal layout - can be configured via WindowDef later
                    let widget =
                        dashboard::Dashboard::new(name, dashboard::DashboardLayout::Horizontal);
                    self.widget_manager.dashboard_widgets.insert(name.clone(), widget);
                }

                // Update widget
                if let Some(widget) = self.widget_manager.dashboard_widgets.get_mut(name) {
                    let indicator_values = indicators.clone();

                    // Apply configuration
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        widget.set_border_config(
                            window_def.base().show_border,
                            Some(window_def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        widget.set_border_sides(window_def.base().border_sides.clone());
                        widget.set_transparent_background(window_def.base().transparent_background);
                        widget.set_background_color(colors.background.clone());
                        widget.set_content_align(window_def.base().content_align.clone());
                        let title_text = if window_def.base().show_title {
                            window_def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        widget.set_title(title_text);

                        if let crate::config::WindowDef::Dashboard { data, .. } = window_def {
                            widget.set_layout(dashboard::DashboardLayout::from_str(&data.layout));
                            widget.set_spacing(data.spacing);
                            widget.set_hide_inactive(data.hide_inactive);
                            widget.clear_indicators();
                            for def in &data.indicators {
                                let colors = if def.colors.is_empty() {
                                    vec!["#ffffff".to_string()]
                                } else {
                                    def.colors.clone()
                                };
                                widget.add_indicator(def.id.clone(), def.icon.clone(), colors);
                            }
                        }
                    }

                    // Apply values after indicators are configured
                    for (id, value) in indicator_values {
                        widget.set_indicator_value(&id, value);
                    }
                }
            }
        }
    }

    /// Sync tabbed text window data from AppCore to tabbed text widgets
    pub(crate) fn sync_tabbed_text_windows(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Note: Highlights are now applied in core (MessageProcessor)
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::TabbedText(tabbed_content) = &window.content {
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Ensure widget exists - create if needed
                if !self.widget_manager.tabbed_text_windows.contains_key(name) {
                    let tabs: Vec<(String, Vec<String>, bool, bool)> = tabbed_content
                        .tabs
                        .iter()
                        .map(|t| {
                            (
                                t.definition.name.clone(),
                                t.definition.streams.clone(),
                                t.definition.show_timestamps,
                                t.definition.ignore_activity,
                            )
                        })
                        .collect();

                    let max_lines =
                        if let Some(crate::config::WindowDef::TabbedText { data, .. }) = window_def
                        {
                            data.buffer_size
                        } else {
                            1000 // fallback
                        };

                    let widget =
                        tabbed_text_window::TabbedTextWindow::with_tabs(name, tabs, max_lines);
                    // Note: Highlights are now applied in core (MessageProcessor)
                    self.widget_manager.tabbed_text_windows.insert(name.clone(), widget);
                }

                // Apply configuration and sync content
                if let Some(widget) = self.widget_manager.tabbed_text_windows.get_mut(name) {
                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        widget.set_border_config(
                            def.base().show_border,
                            Some(def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        widget.set_border_sides(def.base().border_sides.clone());
                        widget.set_transparent_background(def.base().transparent_background);
                        widget.set_background_color(colors.background.clone());
                        widget.set_content_align(def.base().content_align.clone());
                        widget.apply_window_colors(colors.text.clone(), colors.background.clone());
                        let title_text = if def.base().show_title {
                            def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        widget.set_title(title_text);

                        if let crate::config::WindowDef::TabbedText { data, .. } = def {
                            let tab_position = tabbed_text_window::TabBarPosition::from_str(
                                &data.tab_bar_position,
                            );
                            widget.set_tab_bar_position(tab_position);
                            widget.set_tab_separator(data.tab_separator);
                            widget.set_tab_colors(
                                data.tab_active_color.clone(),
                                data.tab_inactive_color.clone(),
                                data.tab_unread_color.clone(),
                            );
                            if let Some(prefix) = data.tab_unread_prefix.clone() {
                                widget.set_unread_prefix(prefix);
                            }
                        }

                        widget.set_title_position(super::title_position::TitlePosition::from_str(
                            &def.base().title_position,
                        ));
                    }

                    // Set active tab
                    widget.switch_to_tab(tabbed_content.active_tab_index);

                    // Sync content for each tab
                    for (i, tab_state) in tabbed_content.tabs.iter().enumerate() {
                        let ignore_activity = tab_state.definition.ignore_activity;
                        if let Some(text_window) = widget.get_tab_window_mut(i) {
                            text_window
                                .set_show_timestamps(tab_state.definition.show_timestamps);
                            text_window
                                .set_timestamp_position(tab_state.definition.timestamp_position);
                            let tab_sync_key = format!("{}:{}", name, tab_state.definition.name);
                            let last_synced_gen = self
                                .widget_manager
                                .last_synced_generation
                                .get(&tab_sync_key)
                                .copied()
                                .unwrap_or(0);
                            let current_gen = tab_state.content.generation;

                            if current_gen > last_synced_gen {
                                let gen_delta = (current_gen - last_synced_gen) as usize;
                                let needs_full_resync =
                                    gen_delta > tab_state.content.lines.len();
                                let mut lines_added = 0usize;

                                if needs_full_resync {
                                    text_window.clear();
                                }

                                let lines_to_add = if needs_full_resync {
                                    tab_state.content.lines.len()
                                } else {
                                    gen_delta.min(tab_state.content.lines.len())
                                };

                                let skip_count =
                                    tab_state.content.lines.len().saturating_sub(lines_to_add);
                                for line in tab_state.content.lines.iter().skip(skip_count) {
                                    lines_added = lines_added.saturating_add(1);
                                    // Set the stream for this line so stream-filtered highlights work
                                    text_window.set_current_stream(&line.stream);

                                    for segment in &line.segments {
                                        // TextWindow now uses the same SpanType and LinkData from data module
                                        let styled_text = text_window::StyledText {
                                            content: segment.text.clone(),
                                            fg: segment
                                                .fg
                                                .as_ref()
                                                .and_then(|hex| parse_hex_color(hex).ok()),
                                            bg: segment
                                                .bg
                                                .as_ref()
                                                .and_then(|hex| parse_hex_color(hex).ok()),
                                            bold: segment.bold,
                                            span_type: segment.span_type,  // Direct use, no conversion needed
                                            link_data: segment.link_data.clone(),  // Direct use, no conversion needed
                                        };
                                        text_window.add_text(styled_text);
                                    }
                                    text_window.finish_line(window.position.width);
                                }
                                // Apply ignore flag before unread handling so unread is skipped when ignored
                                widget.set_tab_ignore_activity(i, ignore_activity);

                                self.widget_manager
                                    .last_synced_generation
                                    .insert(tab_sync_key, current_gen);

                                if i != tabbed_content.active_tab_index
                                    && lines_added > 0
                                    && !ignore_activity
                                {
                                    widget.mark_tab_unread(i, lines_added);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Sync compass widget data from AppCore to compass widgets
    pub(crate) fn sync_compass_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Compass(compass_data) = &window.content {
                // Ensure widget exists
                if !self.widget_manager.compass_widgets.contains_key(name) {
                    let widget = compass::Compass::new(name);
                    self.widget_manager.compass_widgets.insert(name.clone(), widget);
                }

                // Update widget
                if let Some(widget) = self.widget_manager.compass_widgets.get_mut(name) {
                    widget.set_directions(compass_data.directions.clone());

                    // Apply configuration
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        widget.set_border_config(
                            window_def.base().show_border,
                            Some(window_def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        widget.set_border_sides(window_def.base().border_sides.clone());
                        widget.set_transparent_background(window_def.base().transparent_background);
                        widget.set_background_color(colors.background.clone());
                        widget.set_content_align(window_def.base().content_align.clone());
                        let title_text = if window_def.base().show_title {
                            window_def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        widget.set_title(title_text);

                        // Apply compass-specific colors if configured
                        if let crate::config::WindowDef::Compass { data, .. } = window_def {
                            let active_color = normalize_color(&data.active_color).or_else(|| {
                                color_to_hex_string(&theme.window_border_focused)
                                    .or_else(|| color_to_hex_string(&theme.window_border))
                            });
                            let inactive_color =
                                normalize_color(&data.inactive_color).or_else(|| {
                                    blend_colors_hex(
                                        &theme.window_background,
                                        &theme.text_secondary,
                                        0.25,
                                    )
                                    .or_else(|| color_to_hex_string(&theme.text_secondary))
                                });
                            widget.set_colors(active_color, inactive_color);
                        }
                    }
                }
            }
        }
    }

    /// Sync injury doll widget data from AppCore to injury doll widgets
    pub(crate) fn sync_injury_doll_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::InjuryDoll(injury_data) = &window.content {
                // Ensure widget exists
                if !self.widget_manager.injury_doll_widgets.contains_key(name) {
                    let widget = injury_doll::InjuryDoll::new(name);
                    self.widget_manager.injury_doll_widgets.insert(name.clone(), widget);
                }

                // Update widget
                if let Some(widget) = self.widget_manager.injury_doll_widgets.get_mut(name) {
                    // Update all injuries
                    for (body_part, level) in &injury_data.injuries {
                        widget.set_injury(body_part.clone(), *level);
                    }

                    // Apply configuration
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        widget.set_border_config(
                            window_def.base().show_border,
                            Some(window_def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        widget.set_border_sides(window_def.base().border_sides.clone());
                        widget.set_transparent_background(window_def.base().transparent_background);
                        widget.set_background_color(colors.background.clone());
                        let title_text = if window_def.base().show_title {
                            window_def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        widget.set_title(title_text);
                        widget.set_content_align(window_def.base().content_align.clone());

                        // Apply injury doll color configuration if specified
                        if let crate::config::WindowDef::InjuryDoll { data, .. } = window_def {
                            let resolved_default = normalize_color(&data.injury_default_color)
                                .or_else(|| color_to_hex_string(&theme.injury_default_color))
                                .unwrap_or_else(|| "#333333".to_string());
                            // Build colors vec with defaults if not specified
                            let colors = vec![
                                resolved_default,
                                data.injury1_color
                                    .clone()
                                    .unwrap_or_else(|| "#aa5500".to_string()),
                                data.injury2_color
                                    .clone()
                                    .unwrap_or_else(|| "#ff8800".to_string()),
                                data.injury3_color
                                    .clone()
                                    .unwrap_or_else(|| "#ff0000".to_string()),
                                data.scar1_color
                                    .clone()
                                    .unwrap_or_else(|| "#999999".to_string()),
                                data.scar2_color
                                    .clone()
                                    .unwrap_or_else(|| "#777777".to_string()),
                                data.scar3_color
                                    .clone()
                                    .unwrap_or_else(|| "#555555".to_string()),
                            ];
                            widget.set_colors(colors);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn sync_performance_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        use crate::data::WindowContent;

        for (name, window) in &app_core.ui_state.windows {
            if !matches!(window.content, WindowContent::Performance) {
                continue;
            }

            let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);
            let (mut base, mut perf_data) = match window_def {
                Some(crate::config::WindowDef::Performance { base, data }) => (Some(base.clone()), Some(data.clone())),
                Some(def) => (Some(def.base().clone()), None),
                None => (None, None),
            };

            // Fallback to performance template (used by overlay)
            if base.is_none() || perf_data.is_none() {
                if let Some(crate::config::WindowDef::Performance { base: tpl_base, data: tpl_data }) =
                    crate::config::Config::get_window_template("performance")
                {
                    if base.is_none() {
                        base = Some(tpl_base.clone());
                    }
                    if perf_data.is_none() {
                        perf_data = Some(tpl_data.clone());
                    }
                }
            }

            let widget = self
                .widget_manager
                .performance_widgets
                .entry(name.clone())
                .or_insert_with(|| {
                    let w = performance_stats::PerformanceStatsWidget::new();
                    w
                });

            if let Some(base) = base.as_ref() {
                let colors = resolve_window_colors(base, theme);
                let title = if base.show_title {
                    base.title.clone().unwrap_or_default()
                } else {
                    String::new()
                };
                widget.set_title(title);
                widget.set_border_config(
                    base.show_border,
                    Some(base.border_style.clone()),
                    colors.border.clone(),
                );
                widget.set_border_sides(base.border_sides.clone());
                widget.set_background_color(colors.background.clone());
                widget.set_transparent_background(base.transparent_background);
                widget.set_text_color(colors.text.clone());
            }

            if let Some(data) = perf_data.as_ref() {
                widget.apply_flags(data);
            }
        }
    }

    /// Sync hand widget data from AppCore to hand widgets
    pub(crate) fn sync_hand_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find all Hand windows in the UI state
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Hand { item, link } = &window.content {
                // Ensure hand widget exists in cache
                if !self.widget_manager.hand_widgets.contains_key(name) {
                    // Determine hand type based on window name
                    let hand_type = match name.as_str() {
                        "left" | "left_hand" => hand::HandType::Left,
                        "right" | "right_hand" => hand::HandType::Right,
                        "spell" | "spell_hand" => hand::HandType::Spell,
                        _ => hand::HandType::Left, // Default fallback
                    };

                    let widget = hand::Hand::new(name, hand_type);
                    self.widget_manager.hand_widgets.insert(name.clone(), widget);
                }

                // Update hand widget content
                if let Some(hand_widget) = self.widget_manager.hand_widgets.get_mut(name) {
                    // Set content (or empty if None)
                    let content = item.clone().unwrap_or_default();
                    hand_widget.set_content(content);

                    // Apply window configuration from layout
                    if let Some(window_def) =
                        app_core.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let colors = resolve_window_colors(window_def.base(), theme);
                        hand_widget.set_border_config(
                            window_def.base().show_border,
                            Some(window_def.base().border_style.clone()),
                            colors.border.clone(),
                        );
                        hand_widget.set_border_sides(window_def.base().border_sides.clone());
                        let title_text = if window_def.base().show_title {
                            window_def.base().title.clone().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        hand_widget.set_title(title_text);

                        // Apply hand-specific icon/text colors
                        let (data_icon, data_icon_color, data_text_color) =
                            if let crate::config::WindowDef::Hand { data, .. } = window_def {
                                (
                                    data.icon.clone(),
                                    data.icon_color.clone(),
                                    data.text_color.clone(),
                                )
                            } else {
                                (None, None, None)
                            };
                        if let Some(icon) = data_icon {
                            hand_widget.set_icon(icon);
                        }

                        let resolved_text_color =
                            data_text_color.clone().or_else(|| colors.text.clone());
                        hand_widget.set_text_color(resolved_text_color.clone());

                        let icon_color =
                            data_icon_color.clone().or_else(|| resolved_text_color.clone());
                        hand_widget.set_icon_color(icon_color);

                        // Always keep link data for click/drag
                        if let Some(link_ref) = link {
                            hand_widget.set_link_data(Some(link_ref.clone()));
                        } else {
                            hand_widget.set_link_data(None);
                        }

                        // Link/text interaction: if user set a text color, use it for content; otherwise keep link color
                        let mut content_highlight = None;
                        if data_text_color.is_some() {
                            content_highlight = resolved_text_color.clone();
                        } else if link.is_some() {
                            // Resolve palette name to hex value for links preset
                            if let Some(preset) = app_core.config.colors.presets.get("links") {
                                content_highlight = preset.fg.as_ref().map(|c| app_core.config.resolve_palette_color(c));
                            }
                        }
                        hand_widget.set_content_highlight_color(content_highlight);

                        hand_widget.set_background_color(colors.background.clone());
                        hand_widget
                            .set_transparent_background(window_def.base().transparent_background);
                    }
                }
            }
        }
    }

    /// Sync room window data from AppCore to room window widgets
    pub(crate) fn sync_room_windows(
        &mut self,
        app_core: &mut crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        let new_title = if app_core.room_window_dirty {
            Some(self.build_room_title(
                &app_core.room_subtitle,
                &app_core.lich_room_id,
                &app_core.nav_room_id,
            ))
        } else {
            None
        };

        for window_def in app_core
            .layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "room")
        {
            let window_name = window_def.name();
            // Check if widget is new before creating it
            let is_new = !self.widget_manager.room_windows.contains_key(window_name);
            self.ensure_room_window_exists(window_name, window_def);

            if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
                // GameState widgets still need highlight patterns for item highlighting
                if is_new {
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    room_window.set_highlights(highlights);
                    room_window.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                }

                let colors = resolve_window_colors(window_def.base(), theme);
                room_window.set_border_config(
                    window_def.base().show_border,
                    Some(window_def.base().border_style.clone()),
                    colors.border.clone(),
                );
                room_window.set_border_sides(window_def.base().border_sides.clone());
                room_window.set_background_color(colors.background.clone());
                room_window.set_text_color(colors.text.clone());

                if let crate::config::WindowDef::Room { data, .. } = window_def {
                    room_window.set_component_visible("room desc", data.show_desc);
                    room_window.set_component_visible("room objs", data.show_objs);
                    room_window.set_component_visible("room players", data.show_players);
                    room_window.set_component_visible("room exits", data.show_exits);
                    room_window.set_show_name(data.show_name);
                }

                if let Some(ref title) = new_title {
                    room_window.clear_all_components();

                    for (component_id, lines) in &app_core.room_components {
                        room_window.start_component(component_id.clone());

                        for line_segments in lines {
                            for segment in line_segments {
                                room_window.add_segment(segment.clone());
                            }
                            room_window.finish_line();
                        }

                        room_window.finish_component();
                    }

                    room_window.set_title(title.clone());
                }
            }
        }

        if new_title.is_some() {
            app_core.room_window_dirty = false;
        }
    }

    /// Build room window title from room data
    /// Format: "[subtitle - lich_id] (u<nav_id>)"
    /// Example: "[Emberthorn Refuge, Bowery - 33711] (u2022628)"
    fn build_room_title(
        &self,
        subtitle: &Option<String>,
        lich_id: &Option<String>,
        nav_id: &Option<String>,
    ) -> String {
        // Format: [subtitle - lich_room_id] (u_nav_room_id)
        if let Some(ref subtitle_text) = subtitle {
            if let Some(ref lich) = lich_id {
                if let Some(ref nav) = nav_id {
                    format!("[{} - {}] (u{})", subtitle_text, lich, nav)
                } else {
                    format!("[{} - {}]", subtitle_text, lich)
                }
            } else if let Some(ref nav) = nav_id {
                format!("[{}] (u{})", subtitle_text, nav)
            } else {
                format!("[{}]", subtitle_text)
            }
        } else if let Some(ref lich) = lich_id {
            if let Some(ref nav) = nav_id {
                format!("[{}] (u{})", lich, nav)
            } else {
                format!("[{}]", lich)
            }
        } else if let Some(ref nav) = nav_id {
            format!("(u{})", nav)
        } else {
            String::new() // No title to set
        }
    }

    fn parse_progress_display_parts(text: &str, fallback_current: u32, fallback_max: u32) -> (Option<String>, u32, u32) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return (None, fallback_current, fallback_max);
        }

        // Slash form "label 324/326" or "324/326"
        if let Some(slash_pos) = trimmed.rfind('/') {
            let before_slash = &trimmed[..slash_pos];
            let after_slash = &trimmed[slash_pos + 1..];

            let current = Self::last_number(before_slash).unwrap_or(fallback_current);
            let maximum = Self::first_number(after_slash).unwrap_or(fallback_max);

            let label = before_slash
                .find(|c: char| c.is_ascii_digit())
                .map(|idx| before_slash[..idx].trim().to_string())
                .filter(|s| !s.is_empty());

            return (label, current, maximum);
        }

        // Single number/percent form "label 100%" or "100%"
        if let Some(idx) = trimmed.find(|c: char| c.is_ascii_digit()) {
            let current = Self::first_number(&trimmed[idx..]).unwrap_or(fallback_current);
            let label = trimmed[..idx].trim();
            let label_opt = if label.is_empty() {
                None
            } else {
                Some(label.to_string())
            };
            return (label_opt, current, fallback_max);
        }

        // Label-only
        (Some(trimmed.to_string()), fallback_current, fallback_max)
    }

    fn first_number(input: &str) -> Option<u32> {
        input
            .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '%')
            .find_map(|token| token.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
    }

    fn last_number(input: &str) -> Option<u32> {
        input
            .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '%')
            .rev()
            .find_map(|token| token.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
    }

    /// Sync perception windows with app state
    pub(crate) fn sync_perception_windows(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        // Find perception windows in ui_state
        for (name, window) in &app_core.ui_state.windows {
            // Check for Perception content type
            let perc_data = match &window.content {
                crate::data::WindowContent::Perception(data) => Some(data),
                _ => None,
            };

            if let Some(perc_data) = perc_data {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get or create PerceptionWindow for this window
                if !self.widget_manager.perception_windows.contains_key(name) {
                    let title = window_def
                        .and_then(|def| def.base().title.clone())
                        .unwrap_or_else(|| "Perceptions".to_string());
                    let mut perception_window = super::perception::PerceptionWindow::new(title);
                    // GameState widgets still need highlight patterns for item highlighting
                    let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                    perception_window.set_highlights(highlights);
                    perception_window.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                    self.widget_manager.perception_windows.insert(name.clone(), perception_window);
                    tracing::debug!("Created PerceptionWindow widget for '{}'", name);
                }

                // Update configuration and content from WindowDef if present
                if let Some(perception_window) = self.widget_manager.perception_windows.get_mut(name) {
                    if let Some(def) = window_def {
                        let colors = resolve_window_colors(def.base(), theme);
                        perception_window.set_show_border(def.base().show_border);
                        perception_window.set_border_color(colors.border.clone());
                        perception_window.set_background_color(colors.background.clone());
                        perception_window.set_text_color(colors.text.clone());

                        let _title_text = if def.base().show_title {
                            def.base()
                                .title
                                .clone()
                                .unwrap_or_else(|| "Perceptions".to_string())
                        } else {
                            String::new()
                        };
                        // Note: We'd set title here if PerceptionWindow had set_title method
                        // For now, title is set during creation
                    }

                    // Get settings from WindowDef if it's a Perception type
                    let (text_replacements, sort_direction, use_short_spell_names) =
                        if let Some(crate::config::WindowDef::Perception { data, .. }) = window_def {
                            (&data.text_replacements[..], data.sort_direction.clone(), data.use_short_spell_names)
                        } else {
                            (&[][..], crate::config::SortDirection::Descending, false)
                        };

                    // Update compiled replacements cache (only recompiles if changed)
                    perception_window.update_compiled_replacements(text_replacements);
                    let compiled_replacements = perception_window.compiled_replacements();

                    // Process entries: apply spell abbreviations, then custom replacements
                    let mut processed_entries: Vec<crate::data::widget::PerceptionEntry> = perc_data
                        .entries
                        .iter()
                        .filter_map(|entry| {
                            let mut text = entry.raw_text.clone();

                            // Apply short spell names if enabled (BEFORE custom replacements)
                            // Uses Aho-Corasick for O(n) matching instead of O(n * patterns)
                            if use_short_spell_names {
                                text = crate::spell_abbrevs::abbreviate_spells(&text);
                            }

                            // Apply user custom replacements using pre-compiled regex
                            // (no regex compilation here - already compiled and cached)
                            text = crate::config::apply_compiled_text_replacements(&text, compiled_replacements);

                            // If the text becomes empty after replacements, filter it out
                            if text.trim().is_empty() {
                                None
                            } else {
                                Some(crate::data::widget::PerceptionEntry {
                                    raw_text: text,
                                    name: entry.name.clone(),
                                    format: entry.format.clone(),
                                    weight: entry.weight,
                                    link_data: entry.link_data.clone(),
                                })
                            }
                        })
                        .collect();

                    // Re-sort based on configured sort direction
                    match sort_direction {
                        crate::config::SortDirection::Ascending => {
                            processed_entries.sort_by(|a, b| a.weight.cmp(&b.weight));
                        }
                        crate::config::SortDirection::Descending => {
                            processed_entries.sort_by(|a, b| b.weight.cmp(&a.weight));
                        }
                    }

                    perception_window.set_entries(processed_entries);
                }
            }
        }
    }

    /// Sync experience widgets (DR skill training) from AppCore
    pub(crate) fn sync_experience_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::Experience = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get align config from WindowDef
                let align = if let Some(crate::config::WindowDef::Experience { data, .. }) =
                    window_def
                {
                    data.align.clone()
                } else {
                    "left".to_string()
                };

                // Get or create Experience widget for this window
                let experience_widget = self
                    .widget_manager
                    .experience_widgets
                    .entry(name.clone())
                    .or_insert_with(|| {
                        let title = window_def
                            .map(|wd| wd.base().title.clone().unwrap_or_else(|| name.clone()))
                            .unwrap_or_else(|| name.clone());
                        super::experience::Experience::new(&title, &align)
                    });

                // Apply theme colors
                if let Some(def) = window_def {
                    let colors = resolve_window_colors(def.base(), theme);
                    if let Some(border_color) = &colors.border {
                        if let Ok(c) = parse_hex_color(border_color) {
                            experience_widget.set_border_color(c);
                        }
                    }
                    if let Some(text_color) = &colors.text {
                        if let Ok(c) = parse_hex_color(text_color) {
                            experience_widget.set_text_color(c);
                        }
                    }
                    experience_widget.set_background_color(colors.background.clone());
                }

                // Update from game state
                experience_widget.update_from_state(&app_core.game_state.dr_experience);
            }
        }
    }

    /// Sync all GS4Experience widgets from GameState.gs4_experience
    pub(crate) fn sync_gs4_experience_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in &app_core.ui_state.windows {
            if let crate::data::WindowContent::GS4Experience = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get align config from WindowDef
                let align = if let Some(crate::config::WindowDef::GS4Experience { data, .. }) =
                    window_def
                {
                    data.align.clone()
                } else {
                    "center".to_string()
                };

                // Get or create GS4Experience widget for this window
                let gs4_exp_widget = self
                    .widget_manager
                    .gs4_experience_widgets
                    .entry(name.clone())
                    .or_insert_with(|| {
                        let title = window_def
                            .map(|wd| wd.base().title.clone().unwrap_or_else(|| name.clone()))
                            .unwrap_or_else(|| name.clone());
                        super::gs4_experience::GS4Experience::new(&title, &align)
                    });

                // Apply theme colors and config toggles
                if let Some(crate::config::WindowDef::GS4Experience { data, .. }) = window_def {
                    let colors = resolve_window_colors(window_def.unwrap().base(), theme);
                    if let Some(border_color) = &colors.border {
                        if let Ok(c) = parse_hex_color(border_color) {
                            gs4_exp_widget.set_border_color(c);
                        }
                    }
                    if let Some(text_color) = &colors.text {
                        if let Ok(c) = parse_hex_color(text_color) {
                            gs4_exp_widget.set_text_color(c);
                        }
                    }
                    gs4_exp_widget.set_background_color(colors.background.clone());
                    // Apply show toggles from config
                    gs4_exp_widget.set_show_level(data.show_level);
                    gs4_exp_widget.set_show_exp_bar(data.show_exp_bar);
                    // Apply custom bar colors (if configured)
                    if let Some(color_str) = &data.mind_bar_color {
                        if let Ok(c) = parse_hex_color(color_str) {
                            gs4_exp_widget.set_mind_bar_color(c);
                        }
                    }
                    if let Some(color_str) = &data.exp_bar_color {
                        if let Ok(c) = parse_hex_color(color_str) {
                            gs4_exp_widget.set_exp_bar_color(Some(c));
                        }
                    }
                }

                // Update from game state
                gs4_exp_widget.update_from_state(&app_core.game_state.gs4_experience);
            }
        }
    }

    /// Sync all Encumbrance widgets from GameState.encumbrance
    pub(crate) fn sync_encumbrance_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in app_core.ui_state.windows.iter() {
            if let crate::data::WindowContent::Encumbrance = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get align, show_label, and color settings from WindowDef
                let (align, show_label, color_light, color_moderate, color_heavy, color_critical) =
                    if let Some(crate::config::WindowDef::Encumbrance { data, .. }) = window_def {
                        (
                            data.align.clone(),
                            data.show_label,
                            data.color_light.clone(),
                            data.color_moderate.clone(),
                            data.color_heavy.clone(),
                            data.color_critical.clone(),
                        )
                    } else {
                        ("left".to_string(), true, None, None, None, None)
                    };

                // Get or create the widget
                let enc_widget = self
                    .widget_manager
                    .encumbrance_widgets
                    .entry(name.clone())
                    .or_insert_with(|| {
                        let title = window_def
                            .map(|wd| wd.base().title.clone().unwrap_or_else(|| name.clone()))
                            .unwrap_or_else(|| name.clone());
                        super::encumbrance::Encumbrance::new(&title, &align, show_label)
                    });

                // Update show_label on every sync (cached widget may have stale value)
                enc_widget.set_show_label(show_label);

                // Apply theme colors
                if let Some(def) = window_def {
                    let colors = resolve_window_colors(def.base(), theme);
                    if let Some(border_color) = &colors.border {
                        if let Ok(c) = parse_hex_color(border_color) {
                            enc_widget.set_border_color(c);
                        }
                    }
                    if let Some(text_color) = &colors.text {
                        if let Ok(c) = parse_hex_color(text_color) {
                            enc_widget.set_text_color(c);
                        }
                    }
                    enc_widget.set_background_color(colors.background.clone());
                }

                // Apply custom encumbrance bar colors (if configured)
                if let Some(color_str) = &color_light {
                    if let Ok(c) = parse_hex_color(color_str) {
                        enc_widget.set_color_light(c);
                    }
                }
                if let Some(color_str) = &color_moderate {
                    if let Ok(c) = parse_hex_color(color_str) {
                        enc_widget.set_color_moderate(c);
                    }
                }
                if let Some(color_str) = &color_heavy {
                    if let Ok(c) = parse_hex_color(color_str) {
                        enc_widget.set_color_heavy(c);
                    }
                }
                if let Some(color_str) = &color_critical {
                    if let Ok(c) = parse_hex_color(color_str) {
                        enc_widget.set_color_critical(c);
                    }
                }

                // Update from game state
                enc_widget.update_from_state(&app_core.game_state.encumbrance);
            }
        }
    }

    /// Sync MiniVitals widgets - GS4 horizontal 4-bar layout
    pub(crate) fn sync_minivitals_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in app_core.ui_state.windows.iter() {
            if let crate::data::WindowContent::MiniVitals = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get display options and bar colors from WindowDef
                let (numbers_only, current_only, health_color, mana_color, stamina_color, spirit_color) =
                    if let Some(crate::config::WindowDef::MiniVitals { data, .. }) = window_def {
                        (
                            data.numbers_only,
                            data.current_only,
                            data.health_color.clone(),
                            data.mana_color.clone(),
                            data.stamina_color.clone(),
                            data.spirit_color.clone(),
                        )
                    } else {
                        (false, false, None, None, None, None)
                    };

                // Get show_border from WindowDef
                let show_border = window_def
                    .map(|wd| wd.base().show_border)
                    .unwrap_or(false);

                // Get or create the widget
                let mv_widget = self
                    .widget_manager
                    .minivitals_widgets
                    .entry(name.clone())
                    .or_insert_with(|| {
                        let title = window_def
                            .map(|wd| wd.base().title.clone().unwrap_or_else(|| name.clone()))
                            .unwrap_or_else(|| name.clone());
                        super::minivitals::MiniVitals::new(&title, show_border)
                    });

                // Update show_border and display mode on every sync (not just creation)
                mv_widget.set_show_border(show_border);
                mv_widget.set_display_mode(numbers_only, current_only);

                // Apply theme colors
                if let Some(def) = window_def {
                    let colors = resolve_window_colors(def.base(), theme);
                    if let Some(border_color) = &colors.border {
                        if let Ok(c) = parse_hex_color(border_color) {
                            mv_widget.set_border_color(c);
                        }
                    }
                    if let Some(text_color) = &colors.text {
                        if let Ok(c) = parse_hex_color(text_color) {
                            mv_widget.set_text_color(c);
                        }
                    }
                    mv_widget.set_background_color(colors.background.clone());
                }

                // Apply bar colors (with defaults if not specified)
                if let Some(color_str) = &health_color {
                    if let Ok(c) = parse_hex_color(color_str) {
                        mv_widget.set_health_color(c);
                    }
                }
                if let Some(color_str) = &mana_color {
                    if let Ok(c) = parse_hex_color(color_str) {
                        mv_widget.set_mana_color(c);
                    }
                }
                if let Some(color_str) = &stamina_color {
                    if let Ok(c) = parse_hex_color(color_str) {
                        mv_widget.set_stamina_color(c);
                    }
                }
                if let Some(color_str) = &spirit_color {
                    if let Ok(c) = parse_hex_color(color_str) {
                        mv_widget.set_spirit_color(c);
                    }
                }

                // Update from game state
                mv_widget.update_from_state(&app_core.game_state.minivitals);
            }
        }
    }

    /// Sync Betrayer widgets (GS4 blood pool)
    pub(crate) fn sync_betrayer_widgets(
        &mut self,
        app_core: &crate::core::AppCore,
        theme: &crate::theme::AppTheme,
    ) {
        for (name, window) in app_core.ui_state.windows.iter() {
            if let crate::data::WindowContent::Betrayer = &window.content {
                // Look up the WindowDef from layout to get config
                let window_def = app_core.layout.windows.iter().find(|wd| wd.name() == *name);

                // Get options from WindowDef
                let (show_items, bar_color) =
                    if let Some(crate::config::WindowDef::Betrayer { data, .. }) = window_def {
                        (data.show_items, data.bar_color.clone())
                    } else {
                        (true, None)
                    };

                // Get show_border from WindowDef
                let show_border = window_def
                    .map(|wd| wd.base().show_border)
                    .unwrap_or(true);

                // Get or create the widget
                let betrayer_widget = self
                    .widget_manager
                    .betrayer_widgets
                    .entry(name.clone())
                    .or_insert_with(|| {
                        let title = window_def
                            .map(|wd| wd.base().title.clone().unwrap_or_else(|| "Blood Pool".to_string()))
                            .unwrap_or_else(|| "Blood Pool".to_string());
                        super::betrayer::Betrayer::new(&title, show_border)
                    });

                // Update settings on every sync (not just creation)
                betrayer_widget.set_show_border(show_border);
                betrayer_widget.set_show_items(show_items);

                // Apply bar color
                if let Some(color_str) = &bar_color {
                    if let Ok(c) = parse_hex_color(color_str) {
                        betrayer_widget.set_bar_color(c);
                    }
                }

                // Apply theme colors
                if let Some(def) = window_def {
                    let colors = resolve_window_colors(def.base(), theme);
                    if let Some(border_color) = &colors.border {
                        if let Ok(c) = parse_hex_color(border_color) {
                            betrayer_widget.set_border_color(c);
                        }
                    }
                    if let Some(text_color) = &colors.text {
                        if let Ok(c) = parse_hex_color(text_color) {
                            betrayer_widget.set_text_color(c);
                        }
                    }
                    betrayer_widget.set_background_color(colors.background.clone());
                }

                // Apply active item color from config
                if let Some(active_color_str) = &app_core.config.ui.betrayer_active_color {
                    if let Ok(c) = parse_hex_color(active_color_str) {
                        betrayer_widget.set_active_color(c);
                    }
                }

                // Update from game state
                betrayer_widget.update_from_state(&app_core.game_state.betrayer);
            }
        }
    }
}

