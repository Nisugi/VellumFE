//! Window construction and placement: init from layout, adding new
//! windows, position updates, tabbed-tab sync, removal, terminal resize,
//! and layout-to-terminal position calculation.

use super::*;

impl AppCore {
    /// Initialize windows based on current layout
    pub fn init_windows(&mut self, terminal_width: u16, terminal_height: u16) {
        // Preserve command history from existing command_input window
        let preserved_history: Option<Vec<String>> = self
            .ui_state
            .windows
            .get("command_input")
            .and_then(|w| {
                if let WindowContent::CommandInput { history, .. } = &w.content {
                    Some(history.clone())
                } else {
                    None
                }
            });

        // Calculate window positions from layout
        let positions = self.calculate_window_positions(terminal_width, terminal_height);

        // Log all widget types being loaded for debugging
        let widget_types: Vec<_> = self.layout.windows.iter()
            .map(|w| format!("{}:{}", w.name(), w.widget_type()))
            .collect();
        tracing::info!("init_windows: Loading {} windows: {:?}", widget_types.len(), widget_types);

        // Create windows based on layout (only visible ones)
        for window_def in &self.layout.windows {
            // Skip hidden windows (except command_input under the TUI
            // force-show rule — the TUI has no fallback input bar).
            if !window_def.base().visibility.is_shown() {
                let force = self.force_show_command_input
                    && window_def.widget_type() == "command_input";
                if !force {
                    tracing::debug!("Skipping hidden window '{}' during init", window_def.name());
                    continue;
                }
            }

            let position = positions
                .get(window_def.name())
                .cloned()
                .unwrap_or(WindowPosition {
                    x: crate::data::geometry::Col::new(0),
                    y: crate::data::geometry::Row::new(0),
                    width: crate::data::geometry::Width::new(80),
                    height: crate::data::geometry::Height::new(24),
                });

            let widget_type = WidgetType::from_str(window_def.widget_type());

            let title = window_def
                .base()
                .title
                .as_deref()
                .unwrap_or(window_def.name());

            let content = match widget_type {
                WidgetType::Text => {
                    let (buffer_size, streams, compact, show_ts, ts_pos) =
                        if let crate::config::WindowDef::Text { data, .. } = window_def {
                            (
                                data.buffer_size,
                                data.streams.clone(),
                                data.compact,
                                data.show_timestamps,
                                data.timestamp_position
                                    .unwrap_or(self.config.ui.timestamp_position),
                            )
                        } else {
                            (1000, vec![], false, false, self.config.ui.timestamp_position)
                        };
                    let mut text_content = TextContent::new(title, buffer_size);
                    text_content.streams = streams.clone();
                    text_content.compact = compact;
                    text_content.show_timestamps = show_ts;
                    text_content.timestamp_position = ts_pos;

                    // Pre-populate bounty window with cached data on reload
                    if window_def.name().eq_ignore_ascii_case("bounty") && self.game_state.bounty.has_data() {
                        let lines = if compact {
                            &self.game_state.bounty.compact_lines
                        } else {
                            std::slice::from_ref(&self.game_state.bounty.raw_text)
                        };
                        for line_text in lines {
                            text_content.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                                line_text.clone(),
                                "bounty",
                            ));
                        }
                        tracing::info!("Pre-populated bounty window with {} cached lines", lines.len());
                    }

                    // Pre-populate society window with cached data on reload
                    if streams.iter().any(|s| s.eq_ignore_ascii_case("society")) && self.game_state.society.has_data() {
                        for line_text in &self.game_state.society.lines {
                            text_content.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                                line_text.clone(),
                                "society",
                            ));
                        }
                        tracing::info!("Pre-populated society window with {} cached lines", self.game_state.society.lines.len());
                    }

                    WindowContent::Text(text_content)
                }
                WidgetType::TabbedText => {
                    // Extract tab definitions and buffer size from window def
                    if let crate::config::WindowDef::TabbedText { data, .. } = window_def {
                        let global_ts_pos = self.config.ui.timestamp_position;
                        let tabs: Vec<(String, Vec<String>, bool, bool, crate::config::TimestampPosition)> = data
                            .tabs
                            .iter()
                            .map(|tab| {
                                // show_timestamps defaults to false if not explicitly set per-tab
                                let show_ts = tab.show_timestamps.unwrap_or(false);
                                let ignore = tab.ignore_activity.unwrap_or(false);
                                let ts_pos = tab.timestamp_position.unwrap_or(global_ts_pos);
                                (tab.name.clone(), tab.get_streams(), show_ts, ignore, ts_pos)
                            })
                            .collect();
                        WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                            tabs,
                            data.buffer_size,
                        ))
                    } else {
                        // Fallback, though this path should ideally not be taken if config is valid
                        WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                            vec![(
                                "Default".to_string(),
                                vec!["main".to_string()],
                                false, // show_timestamps defaults to false
                                false,
                                crate::config::TimestampPosition::End,
                            )],
                            1000,
                        ))
                    }
                }
                WidgetType::CommandInput => WindowContent::CommandInput {
                    text: String::new(),
                    cursor: 0,
                    history: Vec::new(),
                    history_index: None,
                },
                WidgetType::Progress => {
                    let (label, progress_id, color, numbers_only, current_only) =
                        if let crate::config::WindowDef::Progress { data, .. } = window_def {
                            (
                                data.label.clone().unwrap_or_else(|| title.to_string()),
                                data.id
                                    .clone()
                                    .unwrap_or_else(|| window_def.name().to_string()),
                                data.color.clone(),
                                data.numbers_only,
                                data.current_only,
                            )
                        } else {
                            (
                                title.to_string(),
                                window_def.name().to_string(),
                                None,
                                false,
                                false,
                            )
                        };
                    WindowContent::Progress(ProgressData {
                        value: 100,
                        max: 100,
                        label,
                        color,
                        progress_id,
                        numbers_only,
                        current_only,
                    })
                }
                WidgetType::Countdown => {
                    let (label, countdown_id, color, show_when_zero, count_past_zero) =
                        if let crate::config::WindowDef::Countdown { data, .. } = window_def {
                            (
                                data.label
                                    .clone()
                                    .unwrap_or_else(|| title.to_string()),
                                data.id
                                    .clone()
                                    .unwrap_or_else(|| window_def.name().to_string()),
                                data.color.clone(),
                                data.show_when_zero.unwrap_or(false),
                                data.count_past_zero.unwrap_or(false),
                            )
                        } else {
                            (
                                title.to_string(),
                                window_def.name().to_string(),
                                None,
                                false,
                                false,
                            )
                        };

                    WindowContent::Countdown(CountdownData {
                        end_time: 0,
                        label,
                        countdown_id,
                        color,
                        show_when_zero,
                        count_past_zero,
                    })
                }
                WidgetType::Map => WindowContent::Map(crate::data::MapData::default()),
            WidgetType::Compass => WindowContent::Compass(CompassData {
                    directions: Vec::new(),
                }),
                WidgetType::InjuryDoll => WindowContent::InjuryDoll(InjuryDollData::new()),
                WidgetType::Indicator => {
                    let (indicator_id, active_color) =
                        if let crate::config::WindowDef::Indicator { data, .. } = window_def {
                            (
                                data.indicator_id
                                    .clone()
                                    .unwrap_or_else(|| window_def.name().to_string()),
                                data.active_color.clone(),
                            )
                        } else {
                            (window_def.name().to_string(), None)
                        };
                    WindowContent::Indicator(IndicatorData {
                        indicator_id,
                        active: false,
                        color: active_color,
                    })
                }
                WidgetType::Performance => WindowContent::Performance,
                WidgetType::Hand => WindowContent::Hand {
                    item: None,
                    link: None,
                },
                WidgetType::Room => WindowContent::Room(RoomContent {
                    name: String::new(),
                    description: Vec::new(),
                    exits: Vec::new(),
                    players: Vec::new(),
                    objects: Vec::new(),
                }),
                WidgetType::Inventory => {
                    let mut content = TextContent::new(title, 10000);
                    content.streams = vec!["inv".to_string()];
                    WindowContent::Inventory(content)
                }
                WidgetType::Reserve => {
                    let mut content = TextContent::new(title, 10000);
                    content.streams = vec!["reserve".to_string()];
                    WindowContent::Reserve(content)
                }
                WidgetType::Spells => {
                    let mut content = TextContent::new(title, 10000);
                    content.streams = vec!["Spells".to_string()];
                    tracing::debug!("init_windows: Creating Spells window '{}' with streams={:?}", title, content.streams);
                    WindowContent::Spells(content)
                }
                WidgetType::ActiveEffects => {
                    // Extract category from window def
                    let category =
                        if let crate::config::WindowDef::ActiveEffects { data, .. } = window_def {
                            data.category.clone()
                        } else {
                            "Unknown".to_string()
                        };
                    WindowContent::ActiveEffects(crate::data::ActiveEffectsContent {
                        category,
                        effects: Vec::new(),
                        generation: 0,
                    })
                }
                WidgetType::Targets => WindowContent::Targets,
                WidgetType::Players => WindowContent::Players,
                WidgetType::MissingSpells => WindowContent::MissingSpells,
                WidgetType::Containers => WindowContent::Containers,
                WidgetType::MultiAccount => WindowContent::MultiAccount,
                WidgetType::Items => WindowContent::Items,
                WidgetType::Container => {
                    // Get container_title from window def if available
                    let container_title = if let crate::config::WindowDef::Container { data, .. } = window_def {
                        data.container_title.clone()
                    } else {
                        String::new()
                    };
                    WindowContent::Container { container_title }
                }
                WidgetType::Dashboard => WindowContent::Dashboard {
                    indicators: Vec::new(),
                },
                WidgetType::Perception => WindowContent::Perception(PerceptionData {
                    entries: Vec::new(),
                    last_update: 0,
                    generation: 0,
                }),
                WidgetType::Experience => WindowContent::Experience,
                WidgetType::GS4Experience => WindowContent::GS4Experience,
                WidgetType::Encumbrance => WindowContent::Encumbrance,
                WidgetType::Quickbar => WindowContent::Quickbar,
                WidgetType::Hotkeybar => {
                    let bar = if let crate::config::WindowDef::Hotkeybar { data, .. } = window_def {
                        data.bar.clone()
                    } else {
                        String::new()
                    };
                    WindowContent::Hotkeybar { bar }
                }
                WidgetType::MiniVitals => WindowContent::MiniVitals,
                WidgetType::Betrayer => WindowContent::Betrayer,
                WidgetType::WebUi => {
                    let page = if let crate::config::WindowDef::WebUi { data, .. } = window_def {
                        data.page.clone()
                    } else {
                        String::new()
                    };
                    WindowContent::WebUi(crate::data::webui::WebUiPanelContent::new(page, title))
                }
                // A resident dialog panel (combat, UberBar) renders from the
                // dialog store by its bound id — see add_new_window's twin arm.
                WidgetType::DialogPanel => {
                    let dialog_id = match window_def {
                        crate::config::WindowDef::DialogPanel { data, .. }
                            if !data.dialog_id.is_empty() =>
                        {
                            data.dialog_id.clone()
                        }
                        _ => window_def
                            .base()
                            .binding
                            .as_ref()
                            .map(|b| b.id().to_string())
                            .unwrap_or_default(),
                    };
                    WindowContent::DialogPanel { dialog_id }
                }
                _ => WindowContent::Empty,
            };

            let window = WindowState {
                name: window_def.name().to_string(),
                widget_type,
                content,
                position,
                visible: true,
                content_align: window_def.base().content_align.clone(),
                focused: false,
                ephemeral: false,
            };

            self.ui_state
                .set_window(window_def.name().to_string(), window);
        }

        // Set default focused window to "main" if it exists (enables scrolling with PageUp/PageDown)
        if self.ui_state.focused_window.is_none() {
            if self.ui_state.windows.contains_key("main") {
                self.ui_state.set_focus(Some("main".to_string()));
                tracing::debug!("Set default focused window to 'main'");
            } else if let Some(first_name) = self.ui_state.windows.keys().next().cloned() {
                // Fall back to first window if main doesn't exist
                self.ui_state.set_focus(Some(first_name.clone()));
                tracing::debug!("Set default focused window to '{}'", first_name);
            }
        }

        // Update text stream subscriber map for routing (uses widget stream configs)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);

        // Populate all spells windows from buffer (spells are sent once at login)
        for window in self.ui_state.windows.values_mut() {
            if let WindowContent::Spells(ref mut content) = window.content {
                self.message_processor.populate_spells_window(content);
            }
        }

        // Restore preserved command history
        if let Some(history) = preserved_history {
            if let Some(window) = self.ui_state.windows.get_mut("command_input") {
                if let WindowContent::CommandInput {
                    history: ref mut h, ..
                } = window.content
                {
                    *h = history;
                }
            }
        }

        self.needs_render = true;
    }

    /// Add a single new window without destroying existing ones
    ///
    /// Uses absolute positioning from window definition with optional delta-based scaling.
    pub fn add_new_window(
        &mut self,
        window_def: &crate::config::WindowDef,
        _terminal_width: u16,
        _terminal_height: u16,
    ) {
        tracing::info!(
            "add_new_window: '{}' ({})",
            window_def.name(),
            window_def.widget_type()
        );

        // Use exact position from window definition
        let base = window_def.base();
        let position = WindowPosition {
            x: base.col,
            y: base.row,
            width: base.cols,
            height: base.rows,
        };

        tracing::debug!(
            "Window '{}' will be created at exact pos=({},{}) size={}x{}",
            window_def.name(),
            position.x.get(),
            position.y.get(),
            position.width.get(),
            position.height.get()
        );

        let is_room_window = window_def.widget_type() == "room";

        let widget_type = WidgetType::from_str(window_def.widget_type());

        let title = window_def
            .base()
            .title
            .as_deref()
            .unwrap_or("");

        let content = match widget_type {
            WidgetType::Text => {
                let (buffer_size, streams, compact, show_ts, ts_pos) =
                    if let crate::config::WindowDef::Text { data, .. } = window_def {
                        (
                            data.buffer_size,
                            data.streams.clone(),
                            data.compact,
                            data.show_timestamps,
                            data.timestamp_position
                                .unwrap_or(self.config.ui.timestamp_position),
                        )
                    } else {
                        (1000, vec![], false, false, self.config.ui.timestamp_position)
                    };
                let mut text_content = TextContent::new(title, buffer_size);
                text_content.streams = streams;
                text_content.compact = compact;
                text_content.show_timestamps = show_ts;
                text_content.timestamp_position = ts_pos;

                // For bounty windows: pre-populate with buffered bounty data if available
                if window_def.name().eq_ignore_ascii_case("bounty") && self.game_state.bounty.has_data() {
                    // Use compact lines if window is in compact mode, otherwise raw text
                    let lines = if compact {
                        &self.game_state.bounty.compact_lines
                    } else {
                        // For non-compact, use raw text as single line
                        std::slice::from_ref(&self.game_state.bounty.raw_text)
                    };

                    for line_text in lines {
                        text_content.add_line(crate::data::widget::StyledLine::from_text_with_stream(
                            line_text.clone(),
                            "bounty",
                        ));
                    }
                    tracing::info!(
                        "Pre-populated bounty window with {} buffered lines",
                        lines.len()
                    );
                }

                WindowContent::Text(text_content)
            }
            WidgetType::TabbedText => {
                // Extract tab definitions and buffer size from window def
                if let crate::config::WindowDef::TabbedText { data, .. } = window_def {
                    let global_ts_pos = self.config.ui.timestamp_position;
                    let tabs: Vec<(String, Vec<String>, bool, bool, crate::config::TimestampPosition)> = data
                        .tabs
                        .iter()
                        .map(|tab| {
                            // show_timestamps defaults to false if not explicitly set per-tab
                            let show_ts = tab.show_timestamps.unwrap_or(false);
                            let ignore = tab.ignore_activity.unwrap_or(false);
                            let ts_pos = tab.timestamp_position.unwrap_or(global_ts_pos);
                            (tab.name.clone(), tab.get_streams(), show_ts, ignore, ts_pos)
                        })
                        .collect();
                    WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                        tabs,
                        data.buffer_size,
                    ))
                } else {
                    // Fallback if window_def is wrong type
                    WindowContent::TabbedText(crate::data::TabbedTextContent::new(
                        vec![(
                            "Default".to_string(),
                            vec!["main".to_string()],
                            false, // show_timestamps defaults to false
                            false,
                            crate::config::TimestampPosition::End,
                        )],
                        5000,
                    ))
                }
            }
            WidgetType::CommandInput => WindowContent::CommandInput {
                text: String::new(),
                cursor: 0,
                history: Vec::new(),
                history_index: None,
            },
            WidgetType::Progress => {
                let (label, progress_id, color, numbers_only, current_only) =
                    if let crate::config::WindowDef::Progress { data, .. } = window_def {
                        (
                            data.label.clone().unwrap_or_else(|| title.to_string()),
                            data.id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                            data.color.clone(),
                            data.numbers_only,
                            data.current_only,
                        )
                    } else {
                        (
                            title.to_string(),
                            window_def.name().to_string(),
                            None,
                            false,
                            false,
                        )
                    };
                WindowContent::Progress(ProgressData {
                    value: 100,
                    max: 100,
                    label,
                    color,
                    progress_id,
                    numbers_only,
                    current_only,
                })
            }
            WidgetType::Countdown => {
                let (label, countdown_id, color, show_when_zero, count_past_zero) =
                    if let crate::config::WindowDef::Countdown { data, .. } = window_def {
                        (
                            data.label.clone().unwrap_or_else(|| title.to_string()),
                            data.id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                            data.color.clone(),
                            data.show_when_zero.unwrap_or(false),
                            data.count_past_zero.unwrap_or(false),
                        )
                    } else {
                        (
                            title.to_string(),
                            window_def.name().to_string(),
                            None,
                            false,
                            false,
                        )
                    };
                WindowContent::Countdown(CountdownData {
                    end_time: 0,
                    label,
                    countdown_id,
                    color,
                    show_when_zero,
                    count_past_zero,
                })
            }
            WidgetType::Map => WindowContent::Map(crate::data::MapData::default()),
            WidgetType::Compass => WindowContent::Compass(CompassData {
                directions: Vec::new(),
            }),
            WidgetType::InjuryDoll => WindowContent::InjuryDoll(InjuryDollData::new()),
            WidgetType::Indicator => {
                let (indicator_id, active_color) =
                    if let crate::config::WindowDef::Indicator { data, .. } = window_def {
                        (
                            data.indicator_id
                                .clone()
                                .unwrap_or_else(|| window_def.name().to_string()),
                            data.active_color.clone(),
                        )
                    } else {
                        (window_def.name().to_string(), None)
                    };
                WindowContent::Indicator(IndicatorData {
                    indicator_id,
                    active: false,
                    color: active_color,
                })
            }
            WidgetType::Perception => WindowContent::Perception(PerceptionData {
                entries: Vec::new(),
                last_update: 0,
                generation: 0,
            }),
            WidgetType::Performance => WindowContent::Performance,
            WidgetType::Hand => WindowContent::Hand {
                item: None,
                link: None,
            },
            WidgetType::Room => WindowContent::Room(RoomContent {
                name: String::new(),
                description: Vec::new(),
                exits: Vec::new(),
                players: Vec::new(),
                objects: Vec::new(),
            }),
            WidgetType::Inventory => {
                let mut content = TextContent::new(title, 0);
                content.streams = vec!["inv".to_string()];
                WindowContent::Inventory(content)
            }
            WidgetType::Reserve => {
                let mut content = TextContent::new(title, 0);
                content.streams = vec!["reserve".to_string()];
                WindowContent::Reserve(content)
            }
            WidgetType::Spells => {
                let mut content = TextContent::new(title, 0);
                content.streams = vec!["Spells".to_string()];
                WindowContent::Spells(content)
            }
            WidgetType::ActiveEffects => {
                // Extract category from window def
                let category =
                    if let crate::config::WindowDef::ActiveEffects { data, .. } = window_def {
                        data.category.clone()
                    } else {
                        "Unknown".to_string()
                    };
                WindowContent::ActiveEffects(crate::data::ActiveEffectsContent {
                    category,
                    effects: Vec::new(),
                    generation: 0,
                })
            }
            WidgetType::Targets => WindowContent::Targets,
            WidgetType::Players => WindowContent::Players,
            WidgetType::MissingSpells => WindowContent::MissingSpells,
            WidgetType::Containers => WindowContent::Containers,
            WidgetType::MultiAccount => WindowContent::MultiAccount,
            WidgetType::Items => WindowContent::Items,
            WidgetType::Container => {
                // Get container_title from window def if available
                let container_title = if let crate::config::WindowDef::Container { data, .. } = window_def {
                    data.container_title.clone()
                } else {
                    String::new()
                };
                WindowContent::Container { container_title }
            }
            WidgetType::Dashboard => WindowContent::Dashboard {
                indicators: Vec::new(),
            },
            WidgetType::Experience => WindowContent::Experience,
            WidgetType::GS4Experience => WindowContent::GS4Experience,
            WidgetType::Encumbrance => WindowContent::Encumbrance,
            WidgetType::Quickbar => WindowContent::Quickbar,
            WidgetType::Hotkeybar => {
                let bar = if let crate::config::WindowDef::Hotkeybar { data, .. } = window_def {
                    data.bar.clone()
                } else {
                    String::new()
                };
                WindowContent::Hotkeybar { bar }
            }
            WidgetType::MiniVitals => WindowContent::MiniVitals,
            WidgetType::Betrayer => WindowContent::Betrayer,
            WidgetType::WebUi => {
                let page = if let crate::config::WindowDef::WebUi { data, .. } = window_def {
                    data.page.clone()
                } else {
                    String::new()
                };
                WindowContent::WebUi(crate::data::webui::WebUiPanelContent::new(page, title))
            }
            // A resident dialog panel (combat, UberBar) renders from the
            // dialog store by its bound id. Without this arm the window fell
            // through to Empty and rendered blank even though the store held
            // its bars/labels/skins.
            WidgetType::DialogPanel => {
                let dialog_id = match window_def {
                    crate::config::WindowDef::DialogPanel { data, .. }
                        if !data.dialog_id.is_empty() =>
                    {
                        data.dialog_id.clone()
                    }
                    // Fall back to the binding id (the discovery sets both, but
                    // a hand-authored panel might only carry the binding).
                    _ => window_def
                        .base()
                        .binding
                        .as_ref()
                        .map(|b| b.id().to_string())
                        .unwrap_or_default(),
                };
                WindowContent::DialogPanel { dialog_id }
            }
            _ => WindowContent::Empty,
        };

        let window = WindowState {
            name: window_def.name().to_string(),
            widget_type,
            content,
            position: position.clone(),
            visible: true,
            content_align: window_def.base().content_align.clone(),
            focused: false,
            ephemeral: false,
        };

        self.ui_state
            .set_window(window_def.name().to_string(), window);
        self.needs_render = true;

        // Clear inventory cache if this is an inventory window to force initial render
        if window_def.widget_type() == "inventory" {
            self.message_processor.clear_inventory_cache();
        }

        // Same for reserve windows - force the next reserve update to render
        if window_def.widget_type() == "reserve" {
            self.message_processor.clear_reserve_cache();
        }

        // Populate spells window from buffer if this is a spells window
        // Spells are sent once at login, so we populate immediately from buffer
        if window_def.widget_type() == "spells" {
            if let Some(window) = self.ui_state.windows.get_mut(window_def.name()) {
                if let WindowContent::Spells(ref mut content) = window.content {
                    self.message_processor.populate_spells_window(content);
                }
            }
        }

        // Set dirty flag for room windows to trigger sync in TUI frontend
        if is_room_window {
            self.room_window_dirty = true;
        }

        tracing::info!(
            "Created new window '{}' at ({}, {}) size {}x{}",
            window_def.name(),
            position.x.get(),
            position.y.get(),
            position.width.get(),
            position.height.get()
        );

        // Update text stream subscriber map (new window may have stream subscriptions)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
    }

    /// Update an existing window's position without destroying content
    /// Update an existing window's position from window definition (uses exact positions, no scaling)
    ///
    /// This is called when editing a window via the window editor. It applies the exact
    /// position from the window definition to the UI state without any scaling.
    pub fn update_window_position(
        &mut self,
        window_def: &crate::config::WindowDef,
        _terminal_width: u16,
        _terminal_height: u16,
    ) {
        let base = window_def.base();
        let position = WindowPosition {
            x: base.col,
            y: base.row,
            width: base.cols,
            height: base.rows,
        };

        if let Some(window_state) = self.ui_state.windows.get_mut(window_def.name()) {
            window_state.position = position.clone();
            self.needs_render = true;
            tracing::info!(
                "Updated window '{}' to EXACT position ({}, {}) size {}x{}",
                window_def.name(),
                position.x.get(),
                position.y.get(),
                position.width.get(),
                position.height.get()
            );
        }
    }

    /// Sync tabbed window tabs from layout definition.
    /// Called after window editor saves changes to a TabbedText window.
    /// Returns true if structural changes occurred (requiring widget cache reset).
    pub fn sync_tabbed_window_tabs(&mut self, window_name: &str) -> bool {
        // Find the layout definition
        let window_def = self.layout.windows.iter().find(|w| w.name() == window_name);
        let Some(crate::config::WindowDef::TabbedText { data, base: _ }) = window_def else {
            return false;
        };

        // Get the TabbedTextContent from ui_state
        let Some(window) = self.ui_state.windows.get_mut(window_name) else {
            return false;
        };
        let crate::data::WindowContent::TabbedText(tabbed_content) = &mut window.content else {
            return false;
        };

        // Build new tab definitions from layout
        let global_ts_pos = self.config.ui.timestamp_position;
        let new_tabs: Vec<_> = data
            .tabs
            .iter()
            .map(|tab| {
                let show_ts = tab.show_timestamps.unwrap_or(false);
                let ignore = tab.ignore_activity.unwrap_or(false);
                let ts_pos = tab.timestamp_position.unwrap_or(global_ts_pos);
                (tab.name.clone(), tab.get_streams(), show_ts, ignore, ts_pos)
            })
            .collect();

        // Update and return whether structural change occurred
        let changed = tabbed_content.update_tabs(new_tabs, data.buffer_size);
        if changed {
            tracing::info!("Updated tabs for window '{}'", window_name);
            // Tab streams changed - keep the routing index in sync
            self.message_processor
                .update_text_stream_subscribers(&self.ui_state);
        }
        changed
    }

    /// Remove a window from UI state
    pub fn remove_window(&mut self, name: &str) {
        self.ui_state.remove_window(name);
        self.needs_render = true;
        tracing::info!("Removed window '{}'", name);

        // Update text stream subscriber map (removed window may have had stream subscriptions)
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
    }

    /// Handle terminal resize
    pub fn resize(&mut self, width: u16, height: u16) {
        // Recalculate all window positions
        let positions = self.calculate_window_positions(width, height);

        // Update all window positions
        for (name, position) in positions {
            if let Some(window) = self.ui_state.get_window_mut(&name) {
                window.position = position;
            }
        }

        self.needs_render = true;
    }

    /// Calculate window positions based on layout and terminal size
    pub(super) fn calculate_window_positions(
        &self,
        _width: u16,
        _height: u16,
    ) -> HashMap<String, WindowPosition> {
        let mut positions = HashMap::new();

        // Use exact layout file values (row, col, rows, cols) without any scaling
        // Windows may be offscreen if terminal is smaller than saved layout size
        // User can manually run .resize if they want to redistribute windows

        for window_def in &self.layout.windows {
            // Use exact position and size from layout
            let mut window_width = window_def.base().cols;
            let mut window_height = window_def.base().rows;

            // Apply min/max constraints from window settings
            if let Some(min_cols) = window_def.base().min_cols {
                if window_width.get() < min_cols {
                    tracing::debug!(
                        "Window '{}': enforcing min_cols={} (was {})",
                        window_def.name(),
                        min_cols,
                        window_width.get()
                    );
                    window_width = crate::data::geometry::Width::new(min_cols);
                }
            }
            if let Some(max_cols) = window_def.base().max_cols {
                if window_width.get() > max_cols {
                    tracing::debug!(
                        "Window '{}': enforcing max_cols={} (was {})",
                        window_def.name(),
                        max_cols,
                        window_width.get()
                    );
                    window_width = crate::data::geometry::Width::new(max_cols);
                }
            }
            if let Some(min_rows) = window_def.base().min_rows {
                if window_height.get() < min_rows {
                    tracing::debug!(
                        "Window '{}': enforcing min_rows={} (was {})",
                        window_def.name(),
                        min_rows,
                        window_height.get()
                    );
                    window_height = crate::data::geometry::Height::new(min_rows);
                }
            }
            if let Some(max_rows) = window_def.base().max_rows {
                if window_height.get() > max_rows {
                    tracing::debug!(
                        "Window '{}': enforcing max_rows={} (was {})",
                        window_def.name(),
                        max_rows,
                        window_height.get()
                    );
                    window_height = crate::data::geometry::Height::new(max_rows);
                }
            }

            tracing::debug!(
                "Window '{}': pos=({},{}) size={}x{}",
                window_def.name(),
                window_def.base().col.get(),
                window_def.base().row.get(),
                window_width.get(),
                window_height.get()
            );

            positions.insert(
                window_def.name().to_string(),
                WindowPosition {
                    x: window_def.base().col,
                    y: window_def.base().row,
                    width: window_width,
                    height: window_height,
                },
            );
        }

        positions
    }
}
