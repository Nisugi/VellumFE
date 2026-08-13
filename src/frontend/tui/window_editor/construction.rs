//! WindowEditor construction: the per-widget-type field order, textarea
//! seeding from an existing def or a fresh template, and auto-naming for
//! new windows.

use super::*;

impl WindowEditor {
    /// Set the window name input and underlying WindowDef name.
    pub fn set_name(&mut self, name: &str) {
        self.name_input = Self::create_textarea();
        self.name_input.insert_str(name);
        self.window_def.base_mut().name = name.to_string();
    }

    pub(super) fn create_textarea() -> TextArea<'static> {
        let mut ta = TextArea::default();
        ta.set_cursor_line_style(Style::default());
        ta.set_max_histories(0);
        ta
    }

    pub(super) fn indicator_templates() -> Vec<IndicatorItem> {
        let mut templates = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for template_name in crate::core::local_catalog::all_seed_keys() {
            if let Some(crate::config::WindowDef::Indicator { data, .. }) =
                crate::core::local_catalog::seed(&template_name)
            {
                let id = data
                    .indicator_id
                    .clone()
                    .unwrap_or_else(|| template_name.to_string());
                let key = id.to_lowercase();
                if seen.contains(&key) {
                    continue;
                }

                let icon = data.icon.unwrap_or_default();
                let inactive = data
                    .inactive_color
                    .unwrap_or_else(|| "#555555".to_string());
                let active = data.active_color.unwrap_or_else(|| "#00ff00".to_string());

                seen.insert(key);
                templates.push(IndicatorItem {
                    id,
                    icon,
                    colors: vec![inactive, active],
                    stack: String::new(),
                    enabled: false,
                });
            }
        }

        templates.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
        templates
    }

    pub(super) fn indicators_from_layout(layout: &crate::config::Layout) -> Vec<IndicatorItem> {
        // Start with all templates (disabled by default)
        let mut items = Self::indicator_templates();
        let mut index: std::collections::HashMap<String, usize> = items
            .iter()
            .enumerate()
            .map(|(idx, ind)| (ind.id.to_lowercase(), idx))
            .collect();

        for window in &layout.windows {
            if let crate::config::WindowDef::Indicator { data, .. } = window {
                let id = data
                    .indicator_id
                    .clone()
                    .unwrap_or_else(|| window.name().to_string());
                let icon = data.icon.clone().unwrap_or_default();
                let inactive = data
                    .inactive_color
                    .clone()
                    .unwrap_or_else(|| "#555555".to_string());
                let active = data
                    .active_color
                    .clone()
                    .unwrap_or_else(|| "#00ff00".to_string());
                let key = id.to_lowercase();
                if let Some(idx) = index.get(&key).copied() {
                    let item = &mut items[idx];
                    if !icon.is_empty() {
                        item.icon = icon;
                    }
                    item.colors = vec![inactive, active];
                    item.enabled = true;
                } else {
                    index.insert(key, items.len());
                    items.push(IndicatorItem {
                        id,
                        icon,
                        colors: vec![inactive, active],
                        stack: String::new(),
                        enabled: true,
                    });
                }
            } else if let crate::config::WindowDef::Dashboard { data, .. } = window {
                for ind in &data.indicators {
                    let key = ind.id.to_lowercase();
                    let colors = if ind.colors.is_empty() {
                        vec!["#555555".to_string(), "#00ff00".to_string()]
                    } else {
                        ind.colors.clone()
                    };
                    if let Some(idx) = index.get(&key).copied() {
                        let item = &mut items[idx];
                        if !ind.icon.is_empty() {
                            item.icon = ind.icon.clone();
                        }
                        if !colors.is_empty() {
                            item.colors = colors;
                        }
                        item.stack = ind.stack.clone();
                        item.enabled = true;
                    } else {
                        index.insert(key, items.len());
                        items.push(IndicatorItem {
                            id: ind.id.clone(),
                            icon: ind.icon.clone(),
                            colors,
                            stack: ind.stack.clone(),
                            enabled: true,
                        });
                    }
                }
            }
        }

        items.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
        items
    }

    pub(super) fn textarea_with_value(value: u16) -> TextArea<'static> {
        let mut ta = Self::create_textarea();
        ta.insert_str(value.to_string());
        ta
    }

    /// Build the linear field order used for Tab/Shift+Tab navigation
    pub(super) fn build_field_order_for(window_def: &WindowDef) -> Vec<FieldRef> {
        let mut fields = vec![
            // Identity + geometry (left column)
            FieldRef::Name,
            FieldRef::Title,
            FieldRef::TitlePosition,
            FieldRef::ContentAlign,
            FieldRef::BorderStyle,
            FieldRef::Row,
            FieldRef::Col,
            FieldRef::Rows,
            FieldRef::Cols,
            FieldRef::MinRows,
            FieldRef::MinCols,
            FieldRef::MaxRows,
            FieldRef::MaxCols,
            // Appearance (right column)
            FieldRef::Locked,
            FieldRef::ShowTitle,
            FieldRef::TransparentBg,
            FieldRef::ShowBorder,
            FieldRef::BorderTop,
            FieldRef::BorderBottom,
            FieldRef::BorderLeft,
            FieldRef::BorderRight,
            FieldRef::BgColor,
            FieldRef::BorderColor,
        ];

        // Special section fields appended at end
        match window_def {
            // Multi-account cards are GUI-only and configured there; the TUI
            // editor exposes only the shared base fields.
            WindowDef::MultiAccount { .. } => {}
            WindowDef::CommandInput { .. } => {
                fields.push(FieldRef::PromptIcon);
                fields.push(FieldRef::PromptIconColor);
                fields.push(FieldRef::TextColor);
                fields.push(FieldRef::CursorColor);
                fields.push(FieldRef::CursorBg);
                fields.push(FieldRef::CompletionColor);
            }
            WindowDef::Text { .. } => {
                // Bounty window is special: hide Streams and BufferSize
                let is_bounty = window_def.base().name.eq_ignore_ascii_case("bounty");
                if !is_bounty {
                    fields.push(FieldRef::Streams);
                    fields.push(FieldRef::BufferSize);
                }
                // Speak new lines aloud (TTS opt-in). Text windows carry the
                // lines TTS reads, so the toggle belongs here (mirrors the GUI,
                // which gates tts_speak to text-carrying windows).
                fields.push(FieldRef::TtsSpeak);
                fields.push(FieldRef::Wordwrap);
                fields.push(FieldRef::Timestamps);
                fields.push(FieldRef::TextCompact);
            }
            WindowDef::Inventory { .. } | WindowDef::Reserve { .. } => {
                // No Timestamps here: timestamps are for chatter-style text
                // windows (thoughts, speech), not inventory-style lists.
                fields.push(FieldRef::Streams);
                fields.push(FieldRef::BufferSize);
                fields.push(FieldRef::Wordwrap);
            }
            WindowDef::Quickbar { .. } => {}
            WindowDef::MissingSpells { .. } => {}
            WindowDef::Containers { .. } => {}
            WindowDef::Hotkeybar { .. } => {}
            // Dialog panels have no editable content fields; the game
            // defines their controls. Only geometry (base) applies.
            WindowDef::DialogPanel { .. } => {}
            WindowDef::TabbedText { .. } => {
                fields.push(FieldRef::TabBarPosition);
                fields.push(FieldRef::TabSeparator);
                fields.push(FieldRef::TabUnreadPrefix);
                fields.push(FieldRef::EditTabs);
                fields.push(FieldRef::TabActiveColor);
                fields.push(FieldRef::TabInactiveColor);
                fields.push(FieldRef::TabUnreadColor);
            }
            WindowDef::Room { .. } => {
                fields.push(FieldRef::ShowName);
                fields.push(FieldRef::ShowDesc);
                fields.push(FieldRef::ShowObjs);
                fields.push(FieldRef::ShowPlayers);
                fields.push(FieldRef::ShowExits);
            }
            WindowDef::Progress { .. } => {
                fields.push(FieldRef::ProgressNumbersOnly);
                fields.push(FieldRef::ProgressCurrentOnly);
                fields.push(FieldRef::ProgressId);
                fields.push(FieldRef::TextColor);
                fields.push(FieldRef::ProgressColor);
            }
            WindowDef::Countdown { .. } => {
                fields.push(FieldRef::CountdownIcon);
                fields.push(FieldRef::CountdownId);
                fields.push(FieldRef::CountdownColor);
                fields.push(FieldRef::CountdownBgColor);
            }
            WindowDef::Compass { .. } => {
                fields.push(FieldRef::CompassActiveColor);
                fields.push(FieldRef::CompassInactiveColor);
            }
            // GUI-only widget: no TUI-editable special fields.
            WindowDef::Map { .. } => {}
            WindowDef::InjuryDoll { .. } => {
                // Tab order matches the rendered rows: Wound/Scar pairs,
                // then the uninjured default.
                fields.push(FieldRef::Injury1Color);
                fields.push(FieldRef::Scar1Color);
                fields.push(FieldRef::Injury2Color);
                fields.push(FieldRef::Scar2Color);
                fields.push(FieldRef::Injury3Color);
                fields.push(FieldRef::Scar3Color);
                fields.push(FieldRef::InjuryDefaultColor);
            }
            WindowDef::Indicator { .. } => {
                fields.push(FieldRef::IndicatorId);
                fields.push(FieldRef::IndicatorIcon);
                fields.push(FieldRef::IndicatorActiveColor);
                fields.push(FieldRef::IndicatorInactiveColor);
            }
            WindowDef::Hand { .. } => {
                fields.push(FieldRef::HandIcon);
                fields.push(FieldRef::HandIconColor);
                fields.push(FieldRef::HandTextColor);
            }
            WindowDef::Dashboard { .. } => {
                fields.push(FieldRef::DashboardLayout);
                fields.push(FieldRef::DashboardSpacing);
                fields.push(FieldRef::DashboardHideInactive);
                fields.push(FieldRef::EditIndicators);
            }
            WindowDef::ActiveEffects { .. } => {
                fields.push(FieldRef::ActiveEffectsCategory);
            }
            WindowDef::Targets { .. } => {
                fields.push(FieldRef::EntityId);
                fields.push(FieldRef::TargetsShowAppendages);
                fields.push(FieldRef::TargetsStatusPosition);
            }
            WindowDef::Players { .. } => {
                fields.push(FieldRef::EntityId);
            }
            WindowDef::Items { .. } => {
                fields.push(FieldRef::EntityId);
            }
            WindowDef::Container { .. } => {
                // Could add container_id field in the future
            }
            WindowDef::Performance { .. } => {
                fields.push(FieldRef::EditMetrics);
            }
            WindowDef::Spacer { .. } | WindowDef::Spells { .. } => {}
            WindowDef::Perception { .. } => {
                // Only sort_direction is configurable (stream="percWindow", buffer_size=100 are hardcoded)
                fields.push(FieldRef::PerceptionSortDirection);
                fields.push(FieldRef::PerceptionUseShortSpellNames);
                fields.push(FieldRef::PerceptionTextReplacements);
            }
            WindowDef::Experience { .. } => {
                // Experience widget - alignment is configurable via content_align in base
                // No special fields beyond base settings
            }
            WindowDef::GS4Experience { .. } => {
                // GS4 Experience widget - visibility toggles + bar colors
                fields.push(FieldRef::GS4ExpShowLevel);
                fields.push(FieldRef::GS4ExpShowExpBar);
                fields.push(FieldRef::GS4ExpShowMindBar);
                fields.push(FieldRef::GS4ExpShowTotalExp);
                fields.push(FieldRef::GS4ExpShowAscensionExp);
                fields.push(FieldRef::GS4ExpMindBarColor);
                fields.push(FieldRef::GS4ExpExpBarColor);
            }
            WindowDef::Encumbrance { .. } => {
                // Encumbrance widget - show_label toggle and bar colors
                fields.push(FieldRef::EncumShowLabel);
                fields.push(FieldRef::EncumColorLight);
                fields.push(FieldRef::EncumColorModerate);
                fields.push(FieldRef::EncumColorHeavy);
                fields.push(FieldRef::EncumColorCritical);
            }
            WindowDef::MiniVitals { .. } => {
                // MiniVitals widget display mode toggles
                fields.push(FieldRef::MiniVitalsNumbersOnly);
                fields.push(FieldRef::MiniVitalsCurrentOnly);
                // Bar order and colors editor (handles all 5 bars)
                fields.push(FieldRef::MiniVitalsEditBarOrder);
                fields.push(FieldRef::MiniVitalsDepletedColor);
            }
            WindowDef::Betrayer { .. } => {
                // Betrayer widget - show_items toggle and bar color
                fields.push(FieldRef::BetrayerShowItems);
                fields.push(FieldRef::BetrayerBarColor);
            }
            WindowDef::WebUi { .. } => {
                // Page binding is set by .webui; nothing editable beyond base
            }
        }

        fields
    }

    pub(super) fn refresh_size_inputs(&mut self) {
        // Show total rows/cols (not content rows) - VellumFE style
        self.rows_input = Self::textarea_with_value(self.window_def.base().rows.get().max(1));
        self.cols_input = Self::textarea_with_value(self.window_def.base().cols.get().max(1));

        // Also refresh min/max inputs (they adjust with border changes)
        self.min_rows_input = Self::create_textarea();
        if let Some(min_rows) = self.window_def.base().min_rows {
            self.min_rows_input.insert_str(min_rows.to_string());
        }
        self.min_cols_input = Self::create_textarea();
        if let Some(min_cols) = self.window_def.base().min_cols {
            self.min_cols_input.insert_str(min_cols.to_string());
        }
        self.max_rows_input = Self::create_textarea();
        if let Some(max_rows) = self.window_def.base().max_rows {
            self.max_rows_input.insert_str(max_rows.to_string());
        }
        self.max_cols_input = Self::create_textarea();
        if let Some(max_cols) = self.window_def.base().max_cols {
            self.max_cols_input.insert_str(max_cols.to_string());
        }
    }

    /// Current content alignment value (defaults to first option)
    pub(super) fn current_content_align_value(&self) -> &str {
        self.content_align_input
            .lines()
            .get(0)
            .map(|s| if s.is_empty() { None } else { Some(s.as_str()) })
            .flatten()
            .or_else(|| {
                self.window_def
                    .base()
                    .content_align
                    .as_ref()
                    .map(|s| s.as_str())
            })
            .unwrap_or_else(|| CONTENT_ALIGN_OPTIONS[0])
    }

    pub fn new(window_def: WindowDef) -> Self {
        let mut name_input = Self::create_textarea();
        name_input.insert_str(window_def.name());

        let mut title_input = Self::create_textarea();
        if let Some(ref title) = window_def.base().title {
            title_input.insert_str(title);
        }

        let mut row_input = Self::create_textarea();
        row_input.insert_str(window_def.base().row.get().to_string());

        let mut col_input = Self::create_textarea();
        col_input.insert_str(window_def.base().col.get().to_string());

        // Show total rows/cols (not content rows) - VellumFE style
        // User sets actual widget size; content adjusts based on borders
        let rows_input = Self::textarea_with_value(window_def.base().rows.get().max(1));

        let cols_input = Self::textarea_with_value(window_def.base().cols.get().max(1));

        let mut min_rows_input = Self::create_textarea();
        if let Some(min_rows) = window_def.base().min_rows {
            min_rows_input.insert_str(min_rows.to_string());
        }

        let mut min_cols_input = Self::create_textarea();
        if let Some(min_cols) = window_def.base().min_cols {
            min_cols_input.insert_str(min_cols.to_string());
        }

        let mut max_rows_input = Self::create_textarea();
        if let Some(max_rows) = window_def.base().max_rows {
            max_rows_input.insert_str(max_rows.to_string());
        }

        let mut max_cols_input = Self::create_textarea();
        if let Some(max_cols) = window_def.base().max_cols {
            max_cols_input.insert_str(max_cols.to_string());
        }

        let mut bg_color_input = Self::create_textarea();
        if let Some(ref bg_color) = window_def.base().background_color {
            bg_color_input.insert_str(bg_color);
        }

        let mut border_color_input = Self::create_textarea();
        if let Some(ref border_color) = window_def.base().border_color {
            border_color_input.insert_str(border_color);
        }

        let mut streams_input = Self::create_textarea();
        let mut buffer_size_input = Self::create_textarea();
        let mut text_wordwrap = true;
        let mut text_show_timestamps = false;
        let mut text_compact = false;
        let mut entity_id_input = Self::create_textarea();
        let mut targets_show_arms_count = false;
        let mut targets_status_position = "end".to_string();
        if let crate::config::WindowDef::Text { data, .. } = &window_def {
            streams_input.insert_str(data.streams.join(", "));
            buffer_size_input.insert_str(data.buffer_size.to_string());
            text_wordwrap = data.wordwrap;
            text_show_timestamps = data.show_timestamps;
            text_compact = data.compact;
        }
        if let crate::config::WindowDef::Inventory { data, .. }
        | crate::config::WindowDef::Reserve { data, .. } = &window_def
        {
            streams_input.insert_str(data.streams.join(", "));
            buffer_size_input.insert_str(data.buffer_size.to_string());
            text_wordwrap = data.wordwrap;
            text_show_timestamps = data.show_timestamps;
        }
        if let crate::config::WindowDef::Targets { data, .. } = &window_def {
            entity_id_input.insert_str(&data.entity_id);
            targets_show_arms_count = data.show_body_part_count;
            targets_status_position = data.status_position.clone().unwrap_or_else(|| "end".to_string());
        }
        if let crate::config::WindowDef::Players { data, .. } = &window_def {
            entity_id_input.insert_str(&data.entity_id);
        }

        let mut text_color_input = Self::create_textarea();
        let mut prompt_icon_input = Self::create_textarea();
        let mut prompt_icon_color_input = Self::create_textarea();
        let mut cursor_color_input = Self::create_textarea();
        let mut cursor_bg_input = Self::create_textarea();
        let mut completion_color_input = Self::create_textarea();
        let mut tab_bar_position_input = Self::create_textarea();
        let mut title_position_input = Self::create_textarea();
        title_position_input.insert_str(&window_def.base().title_position);
        let mut tab_active_color_input = Self::create_textarea();
        let mut tab_inactive_color_input = Self::create_textarea();
        let mut tab_unread_color_input = Self::create_textarea();
        let mut tab_unread_prefix_input = Self::create_textarea();
        let mut tab_separator = false;
        let mut progress_id_input = Self::create_textarea();
        let mut progress_color_input = Self::create_textarea();
        let mut countdown_id_input = Self::create_textarea();
        let mut countdown_icon_input = Self::create_textarea();
        let mut countdown_color_input = Self::create_textarea();
        let mut countdown_bg_color_input = Self::create_textarea();
        let mut compass_active_color_input = Self::create_textarea();
        let mut compass_inactive_color_input = Self::create_textarea();
        let mut injury_default_color_input = Self::create_textarea();
        let mut injury1_color_input = Self::create_textarea();
        let mut injury2_color_input = Self::create_textarea();
        let mut injury3_color_input = Self::create_textarea();
        let mut scar1_color_input = Self::create_textarea();
        let mut scar2_color_input = Self::create_textarea();
        let mut scar3_color_input = Self::create_textarea();
        let mut indicator_id_input = Self::create_textarea();
        let mut indicator_icon_input = Self::create_textarea();
        let mut indicator_active_color_input = Self::create_textarea();
        let mut indicator_inactive_color_input = Self::create_textarea();
        let mut active_effects_category_input = Self::create_textarea();
        let mut hand_icon_input = Self::create_textarea();
        let mut hand_icon_color_input = Self::create_textarea();
        let mut hand_text_color_input = Self::create_textarea();
        let mut dashboard_layout_input = Self::create_textarea();
        let mut dashboard_spacing_input = Self::create_textarea();
        let mut dashboard_hide_inactive = false;
        let mut perf_enabled = true;
        let mut perf_show_fps = true;
        let mut perf_show_render_times = true;
        let mut perf_show_ui_times = true;
        let mut perf_show_wrap_times = true;
        let mut perf_show_net = true;
        let mut perf_show_parse = true;
        let mut perf_show_events = true;
        let mut perf_show_cpu = true;
        let mut perf_show_memory = true;
        let mut perf_show_lines = true;
        let mut perf_show_uptime = true;
        let mut perf_show_spike_log = true;
        let mut perf_show_per_window = true;
        let mut perf_sparklines = true;
        let mut show_desc = true;
        let mut show_objs = true;
        let mut show_players = true;
        let mut show_exits = true;
        let mut show_name = false;
        let mut progress_numbers_only = false;
        let mut progress_current_only = false;
        if let Some(ref color) = window_def.base().text_color {
            text_color_input.insert_str(color);
        }
        if let crate::config::WindowDef::CommandInput { data, .. } = &window_def {
            if let Some(ref color) = data.input_text_color {
                text_color_input.insert_str(color);
            }
            if let Some(ref icon) = data.prompt_icon {
                prompt_icon_input.insert_str(icon);
            }
            if let Some(ref color) = data.prompt_icon_color {
                prompt_icon_color_input.insert_str(color);
            }
            if let Some(ref color) = data.cursor_color {
                cursor_color_input.insert_str(color);
            }
            if let Some(ref color) = data.cursor_background_color {
                cursor_bg_input.insert_str(color);
            }
            if let Some(ref color) = data.completion_color {
                completion_color_input.insert_str(color);
            }
        }

        if let crate::config::WindowDef::TabbedText { data, .. } = &window_def {
            tab_bar_position_input.insert_str(&data.tab_bar_position);
            tab_separator = data.tab_separator;
            if let Some(ref c) = data.tab_active_color {
                tab_active_color_input.insert_str(c);
            }
            if let Some(ref c) = data.tab_inactive_color {
                tab_inactive_color_input.insert_str(c);
            }
            if let Some(ref c) = data.tab_unread_color {
                tab_unread_color_input.insert_str(c);
            }
            if let Some(ref prefix) = data.tab_unread_prefix {
                tab_unread_prefix_input.insert_str(prefix);
            }
        }

        if let crate::config::WindowDef::Progress { data, .. } = &window_def {
            if let Some(ref id) = data.id {
                progress_id_input.insert_str(id);
            } else {
                progress_id_input.insert_str(&window_def.base().name);
            }
            if let Some(ref color) = data.color {
                progress_color_input.insert_str(color);
            }
            progress_numbers_only = data.numbers_only;
            progress_current_only = data.current_only;
        }

        if let crate::config::WindowDef::Countdown { data, .. } = &window_def {
            if let Some(ref id) = data.id {
                countdown_id_input.insert_str(id);
            }
            if let Some(icon) = data.icon {
                countdown_icon_input.insert_str(&icon.to_string());
            }
            if let Some(ref color) = data.color {
                countdown_color_input.insert_str(color);
            } else if let Some(ref color) = window_def.base().text_color {
                // Use the template's text color as the default icon color
                countdown_color_input.insert_str(color);
            }
            if let Some(ref color) = data.countdown_background_color {
                countdown_bg_color_input.insert_str(color);
            }
        }

        if let crate::config::WindowDef::Compass { data, .. } = &window_def {
            if let Some(ref c) = data.active_color {
                compass_active_color_input.insert_str(c);
            }
            if let Some(ref c) = data.inactive_color {
                compass_inactive_color_input.insert_str(c);
            }
        }

        if let crate::config::WindowDef::InjuryDoll { data, .. } = &window_def {
            if let Some(ref c) = data.injury_default_color {
                injury_default_color_input.insert_str(c);
            }
            if let Some(ref c) = data.injury1_color {
                injury1_color_input.insert_str(c);
            }
            if let Some(ref c) = data.injury2_color {
                injury2_color_input.insert_str(c);
            }
            if let Some(ref c) = data.injury3_color {
                injury3_color_input.insert_str(c);
            }
            if let Some(ref c) = data.scar1_color {
                scar1_color_input.insert_str(c);
            }
            if let Some(ref c) = data.scar2_color {
                scar2_color_input.insert_str(c);
            }
            if let Some(ref c) = data.scar3_color {
                scar3_color_input.insert_str(c);
            }
        }

        if let crate::config::WindowDef::Hand { data, .. } = &window_def {
            if let Some(ref icon) = data.icon {
                hand_icon_input.insert_str(icon);
            } else {
                // Default icons based on common hand names
                let default_icon = match window_def.base().name.as_str() {
                    "left" | "left_hand" => Some("L:"),
                    "right" | "right_hand" => Some("R:"),
                    "spell" | "spell_hand" => Some("S:"),
                    _ => None,
                };
                if let Some(icon) = default_icon {
                    hand_icon_input.insert_str(icon);
                }
            }
            if let Some(ref c) = data.icon_color {
                hand_icon_color_input.insert_str(c);
            }
            if let Some(ref c) = data.hand_text_color {
                hand_text_color_input.insert_str(c);
            }
        }

        if let crate::config::WindowDef::Indicator { data, .. } = &window_def {
            if let Some(ref id) = data.indicator_id {
                indicator_id_input.insert_str(id);
            } else {
                indicator_id_input.insert_str(&window_def.base().name);
            }
            if let Some(ref icon) = data.icon {
                indicator_icon_input.insert_str(icon);
            }
            if let Some(ref color) = data.active_color {
                indicator_active_color_input.insert_str(color);
            }
            if let Some(ref color) = data.inactive_color {
                indicator_inactive_color_input.insert_str(color);
            }
        }

        if let crate::config::WindowDef::ActiveEffects { data, .. } = &window_def {
            active_effects_category_input.insert_str(&data.category);
        }

        if let crate::config::WindowDef::Dashboard { data, .. } = &window_def {
            dashboard_layout_input.insert_str(&data.layout);
            dashboard_spacing_input.insert_str(data.spacing.to_string());
            dashboard_hide_inactive = data.hide_inactive;
        }

        if let crate::config::WindowDef::Performance { data, .. } = &window_def {
            perf_enabled = data.enabled;
            perf_show_fps = data.show_fps;
            perf_show_render_times = data.show_render_times;
            perf_show_ui_times = data.show_ui_times;
            perf_show_wrap_times = data.show_wrap_times;
            perf_show_net = data.show_net;
            perf_show_parse = data.show_parse;
            perf_show_events = data.show_events;
            perf_show_cpu = data.show_cpu;
            perf_show_memory = data.show_memory;
            perf_show_lines = data.show_lines;
            perf_show_uptime = data.show_uptime;
            perf_show_spike_log = data.show_spike_log;
            perf_show_per_window = data.show_per_window;
            perf_sparklines = data.sparklines;
        }

        if let crate::config::WindowDef::Room { data, .. } = &window_def {
            show_desc = data.show_desc;
            show_objs = data.show_objs;
            show_players = data.show_players;
            show_exits = data.show_exits;
            show_name = data.show_name;
        }

        // Perception widget fields
        // Note: stream and buffer_size are hardcoded - only sort_direction is user-configurable
        let mut perception_sort_direction_input = Self::create_textarea();
        let mut perception_use_short_spell_names = false;

        if let crate::config::WindowDef::Perception { data, .. } = &window_def {
            perception_sort_direction_input.insert_str(match data.sort_direction {
                crate::config::SortDirection::Ascending => "ascending",
                crate::config::SortDirection::Descending => "descending",
            });
            perception_use_short_spell_names = data.use_short_spell_names;
        }

        // Encumbrance widget fields - defaults match widget defaults (green, yellow, orange, red)
        let (
            show_label_encum,
            encum_color_light,
            encum_color_moderate,
            encum_color_heavy,
            encum_color_critical,
        ) = if let crate::config::WindowDef::Encumbrance { data, .. } = &window_def {
            (
                data.show_label,
                data.color_light.clone().unwrap_or_else(|| "#00FF00".to_string()),
                data.color_moderate.clone().unwrap_or_else(|| "#FFFF00".to_string()),
                data.color_heavy.clone().unwrap_or_else(|| "#FFA500".to_string()),
                data.color_critical.clone().unwrap_or_else(|| "#FF0000".to_string()),
            )
        } else {
            (true, "#00FF00".to_string(), "#FFFF00".to_string(), "#FFA500".to_string(), "#FF0000".to_string())
        };

        let mut encum_color_light_input = Self::create_textarea();
        encum_color_light_input.insert_str(&encum_color_light);
        let mut encum_color_moderate_input = Self::create_textarea();
        encum_color_moderate_input.insert_str(&encum_color_moderate);
        let mut encum_color_heavy_input = Self::create_textarea();
        encum_color_heavy_input.insert_str(&encum_color_heavy);
        let mut encum_color_critical_input = Self::create_textarea();
        encum_color_critical_input.insert_str(&encum_color_critical);

        // GS4Experience widget fields - mind bar default cyan, exp bar default empty (theme bg)
        let (
            gs4_exp_show_level,
            gs4_exp_show_exp_bar,
            gs4_exp_show_mind_bar,
            gs4_exp_show_total_exp,
            gs4_exp_show_ascension_exp,
            gs4_exp_mind_bar_color,
            gs4_exp_exp_bar_color,
        ) = if let crate::config::WindowDef::GS4Experience { data, .. } = &window_def {
            (
                data.show_level,
                data.show_exp_bar,
                data.show_mind_bar,
                data.show_total_exp,
                data.show_ascension_exp,
                data.mind_bar_color.clone().unwrap_or_else(|| "#00FFFF".to_string()),
                data.exp_bar_color.clone().unwrap_or_default(), // Empty = theme background
            )
        } else {
            (true, true, true, false, false, "#00FFFF".to_string(), String::new())
        };

        let mut gs4_exp_mind_bar_color_input = Self::create_textarea();
        gs4_exp_mind_bar_color_input.insert_str(&gs4_exp_mind_bar_color);
        let mut gs4_exp_exp_bar_color_input = Self::create_textarea();
        gs4_exp_exp_bar_color_input.insert_str(&gs4_exp_exp_bar_color);

        // MiniVitals widget fields
        let (
            minivitals_numbers_only,
            minivitals_current_only,
            minivitals_health_color,
            minivitals_mana_color,
            minivitals_stamina_color,
            minivitals_spirit_color,
            minivitals_depleted_color,
        ) = if let crate::config::WindowDef::MiniVitals { data, .. } = &window_def {
            (
                data.numbers_only,
                data.current_only,
                data.health_color.clone().unwrap_or_else(|| "#6e0202".to_string()),
                data.mana_color.clone().unwrap_or_else(|| "#08086d".to_string()),
                data.stamina_color.clone().unwrap_or_else(|| "#bd7b00".to_string()),
                data.spirit_color.clone().unwrap_or_else(|| "#6e727c".to_string()),
                // No default: empty means "use the window background"
                data.depleted_color.clone().unwrap_or_default(),
            )
        } else {
            (false, false, "#6e0202".to_string(), "#08086d".to_string(), "#bd7b00".to_string(), "#6e727c".to_string(), String::new())
        };

        let mut minivitals_health_color_input = Self::create_textarea();
        minivitals_health_color_input.insert_str(&minivitals_health_color);
        let mut minivitals_mana_color_input = Self::create_textarea();
        minivitals_mana_color_input.insert_str(&minivitals_mana_color);
        let mut minivitals_stamina_color_input = Self::create_textarea();
        minivitals_stamina_color_input.insert_str(&minivitals_stamina_color);
        let mut minivitals_spirit_color_input = Self::create_textarea();
        minivitals_spirit_color_input.insert_str(&minivitals_spirit_color);
        let mut minivitals_depleted_color_input = Self::create_textarea();
        minivitals_depleted_color_input.insert_str(&minivitals_depleted_color);

        // Betrayer widget fields
        let (betrayer_show_items, betrayer_bar_color) =
            if let crate::config::WindowDef::Betrayer { data, .. } = &window_def {
                (
                    data.show_items,
                    data.bar_color.clone().unwrap_or_else(|| "#8b0000".to_string()),
                )
            } else {
                (true, "#8b0000".to_string())
            };

        let mut betrayer_bar_color_input = Self::create_textarea();
        betrayer_bar_color_input.insert_str(&betrayer_bar_color);

        let mut content_align_input = Self::create_textarea();
        if let Some(ref align) = window_def.base().content_align {
            content_align_input.insert_str(align);
        }

        let field_order = Self::build_field_order_for(&window_def);

        Self {
            popup_x: 0,
            popup_y: 0,
            popup_width: 70,
            popup_height: 20,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            field_order,
            current_field_index: 0,
            focused_field: FieldRef::Name.legacy_field_id(),
            name_input,
            title_input,
            row_input,
            col_input,
            rows_input,
            cols_input,
            min_rows_input,
            min_cols_input,
            max_rows_input,
            max_cols_input,
            bg_color_input,
            border_color_input,
            streams_input,
            buffer_size_input,
            text_wordwrap,
            text_show_timestamps,
            entity_id_input,
            text_color_input,
            prompt_icon_input,
            prompt_icon_color_input,
            cursor_color_input,
            cursor_bg_input,
            completion_color_input,
            content_align_input,
            tab_bar_position_input,
            title_position_input,
            tab_active_color_input,
            tab_inactive_color_input,
            tab_unread_color_input,
            tab_unread_prefix_input,
            tab_separator,
            progress_id_input,
            progress_color_input,
            progress_numbers_only,
            progress_current_only,
            countdown_id_input,
            countdown_icon_input,
            countdown_color_input,
            countdown_bg_color_input,
            compass_active_color_input,
            compass_inactive_color_input,
            injury_default_color_input,
            injury1_color_input,
            injury2_color_input,
            injury3_color_input,
            scar1_color_input,
            scar2_color_input,
            scar3_color_input,
            indicator_id_input,
            indicator_icon_input,
            indicator_active_color_input,
            indicator_inactive_color_input,
            active_effects_category_input,
            hand_icon_input,
            hand_icon_color_input,
            hand_text_color_input,
            dashboard_layout_input,
            dashboard_spacing_input,
            dashboard_hide_inactive,
            perf_enabled,
            show_desc,
            show_objs,
            show_players,
            show_exits,
            show_name,
            perf_show_fps,
            perf_show_render_times,
            perf_show_ui_times,
            perf_show_wrap_times,
            perf_show_net,
            perf_show_parse,
            perf_show_events,
            perf_show_cpu,
            perf_show_memory,
            perf_show_lines,
            perf_show_uptime,
            perf_show_spike_log,
            perf_show_per_window,
            perf_sparklines,
            available_indicators: Vec::new(),
            perception_sort_direction_input,
            perception_use_short_spell_names,
            show_label_encum,
            encum_color_light_input,
            encum_color_moderate_input,
            encum_color_heavy_input,
            encum_color_critical_input,
            gs4_exp_show_level,
            gs4_exp_show_exp_bar,
            gs4_exp_show_mind_bar,
            gs4_exp_show_total_exp,
            gs4_exp_show_ascension_exp,
            gs4_exp_mind_bar_color_input,
            gs4_exp_exp_bar_color_input,
            minivitals_numbers_only,
            minivitals_current_only,
            minivitals_health_color_input,
            minivitals_mana_color_input,
            minivitals_stamina_color_input,
            minivitals_spirit_color_input,
            minivitals_depleted_color_input,
            betrayer_show_items,
            betrayer_bar_color_input,
            text_compact,
            targets_show_arms_count,
            targets_status_position,
            window_def: window_def.clone(),
            original_window_def: window_def,
            is_new: false,
            status_message: "Tab/Shift+Tab: Navigate | Ctrl+S: Save | Esc: Cancel".to_string(),
            tab_editor: None,
            indicator_editor: None,
            performance_metrics_editor: None,
            text_replacements_editor: None,
            bar_order_editor: None,
            stream_picker: None,
            seen_streams: Vec::new(),
            field_click_areas: Vec::new(),
        }
    }

    /// Create editor for a new window from a template
    pub fn new_from_template(template: WindowDef) -> Self {
        // Create editor with template (reuse new() logic)
        let mut editor = Self::new(template);
        // Mark as new so Ctrl+s adds instead of updates
        editor.is_new = true;
        editor
    }

    pub fn new_with_layout(window_def: WindowDef, layout: &crate::config::Layout) -> Self {
        let mut editor = Self::new(window_def);
        editor.available_indicators = Self::indicators_from_layout(layout);
        editor
    }

    pub fn new_window(widget_type: String) -> Self {
        use crate::config::{
            BorderSides, CommandInputWidgetData, PerformanceWidgetData, RoomWidgetData, SpacerWidgetData,
            TextWidgetData, WindowBase, WindowDef,
        };

        // Create base configuration with defaults
        let base = WindowBase {
            name: String::new(),
            row: crate::data::geometry::Row::new(0),
            col: crate::data::geometry::Col::new(0),
            rows: crate::data::geometry::Height::new(10),
            cols: crate::data::geometry::Width::new(40),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: false,
            title: None,
            title_position: "top-left".to_string(),
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            visibility: crate::config::WindowVisibility::Shown,
            binding: None,
            content_align: None,
            tts_speak: false,
            text_size: None,
            font_family: None,
        };

        // Create window_def based on widget type
        let window_def = match widget_type.to_lowercase().as_str() {
            "text" => WindowDef::Text {
                base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: 10000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            },
            "room" => WindowDef::Room {
                base,
                data: RoomWidgetData {
                    buffer_size: 0,
                    show_desc: true,
                    show_objs: true,
                    show_players: true,
                    show_exits: true,
                    show_name: false,
                },
            },
            "command_input" => WindowDef::CommandInput {
                base,
                data: CommandInputWidgetData::default(),
            },
            "spacer" => WindowDef::Spacer {
                base,
                data: SpacerWidgetData {},
            },
            "performance" => WindowDef::Performance {
                base,
                data: PerformanceWidgetData::default(),
            },
            _ => WindowDef::Text {
                base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: 10000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            },
        };

        let name_input = Self::create_textarea();
        let title_input = Self::create_textarea();

        let mut row_input = Self::create_textarea();
        row_input.insert_str("0");

        let mut col_input = Self::create_textarea();
        col_input.insert_str("0");

        // Show total rows/cols (not content rows) - VellumFE style
        let rows_input = Self::textarea_with_value(window_def.base().rows.get().max(1));

        let cols_input = Self::textarea_with_value(window_def.base().cols.get().max(1));

        let min_rows_input = Self::create_textarea();
        let min_cols_input = Self::create_textarea();
        let max_rows_input = Self::create_textarea();
        let max_cols_input = Self::create_textarea();
        let bg_color_input = Self::create_textarea();
        let border_color_input = Self::create_textarea();
        let streams_input = Self::create_textarea();
        let mut buffer_size_input = Self::create_textarea();
        buffer_size_input.insert_str("10000");
        let text_wordwrap = true;
        let text_show_timestamps = false;
        let text_compact = false;
        let entity_id_input = Self::create_textarea();
        let targets_show_arms_count = false;
        let targets_status_position = "end".to_string();
        let text_color_input = Self::create_textarea();
        let prompt_icon_input = Self::create_textarea();
        let prompt_icon_color_input = Self::create_textarea();
        let cursor_color_input = Self::create_textarea();
        let cursor_bg_input = Self::create_textarea();
        let completion_color_input = Self::create_textarea();
        let content_align_input = Self::create_textarea();
        let mut tab_bar_position_input = Self::create_textarea();
        tab_bar_position_input.insert_str("top");
        let mut title_position_input = Self::create_textarea();
        title_position_input.insert_str("top-left");
        let tab_active_color_input = Self::create_textarea();
        let tab_inactive_color_input = Self::create_textarea();
        let tab_unread_color_input = Self::create_textarea();
        let tab_unread_prefix_input = Self::create_textarea();
        let tab_separator = false;
        let mut progress_id_input = Self::create_textarea();
        if let crate::config::WindowDef::Progress { .. } = &window_def {
            progress_id_input.insert_str(&window_def.base().name);
        }
        let progress_color_input = Self::create_textarea();
        let progress_numbers_only = false;
        let progress_current_only = false;
        let countdown_id_input = Self::create_textarea();
        let countdown_icon_input = Self::create_textarea();
        let countdown_color_input = Self::create_textarea();
        let countdown_bg_color_input = Self::create_textarea();
        let compass_active_color_input = Self::create_textarea();
        let compass_inactive_color_input = Self::create_textarea();
        let injury_default_color_input = Self::create_textarea();
        let injury1_color_input = Self::create_textarea();
        let injury2_color_input = Self::create_textarea();
        let injury3_color_input = Self::create_textarea();
        let scar1_color_input = Self::create_textarea();
        let scar2_color_input = Self::create_textarea();
        let scar3_color_input = Self::create_textarea();
        let indicator_id_input = Self::create_textarea();
        let indicator_icon_input = Self::create_textarea();
        let indicator_active_color_input = Self::create_textarea();
        let indicator_inactive_color_input = Self::create_textarea();
        let active_effects_category_input = Self::create_textarea();
        let hand_icon_input = Self::create_textarea();
        let hand_icon_color_input = Self::create_textarea();
        let hand_text_color_input = Self::create_textarea();
        let dashboard_layout_input = Self::create_textarea();
        let dashboard_spacing_input = Self::create_textarea();
        let dashboard_hide_inactive = false;
        let perf_enabled = true;
        let perf_show_fps = true;
        let perf_show_render_times = true;
        let perf_show_ui_times = true;
        let perf_show_wrap_times = true;
        let perf_show_net = true;
        let perf_show_parse = true;
        let perf_show_events = true;
        let perf_show_cpu = true;
        let perf_show_memory = true;
        let perf_show_lines = true;
        let perf_show_uptime = true;
        let perf_show_spike_log = true;
        let perf_show_per_window = true;
        let perf_sparklines = true;
        let show_desc = true;
        let show_objs = true;
        let show_players = true;
        let show_exits = true;
        let show_name = false;

        // Perception widget - default to descending sort, short names off
        let mut perception_sort_direction_input = Self::create_textarea();
        perception_sort_direction_input.insert_str("descending");
        let perception_use_short_spell_names = false;

        let field_order = Self::build_field_order_for(&window_def);

        Self {
            popup_x: 0,
            popup_y: 0,
            popup_width: 70,
            popup_height: 20,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            field_order,
            current_field_index: 0,
            focused_field: FieldRef::Name.legacy_field_id(),
            name_input,
            title_input,
            row_input,
            col_input,
            rows_input,
            cols_input,
            min_rows_input,
            min_cols_input,
            max_rows_input,
            max_cols_input,
            bg_color_input,
            border_color_input,
            streams_input,
            buffer_size_input,
            text_wordwrap,
            text_show_timestamps,
            entity_id_input,
            text_color_input,
            prompt_icon_input,
            prompt_icon_color_input,
            cursor_color_input,
            cursor_bg_input,
            completion_color_input,
            content_align_input,
            tab_bar_position_input,
            title_position_input,
            tab_active_color_input,
            tab_inactive_color_input,
            tab_unread_color_input,
            tab_unread_prefix_input,
            tab_separator,
            progress_id_input,
            progress_color_input,
            progress_numbers_only,
            progress_current_only,
            countdown_id_input,
            countdown_icon_input,
            countdown_color_input,
            countdown_bg_color_input,
            compass_active_color_input,
            compass_inactive_color_input,
            injury_default_color_input,
            injury1_color_input,
            injury2_color_input,
            injury3_color_input,
            scar1_color_input,
            scar2_color_input,
            scar3_color_input,
            indicator_id_input,
            indicator_icon_input,
            indicator_active_color_input,
            indicator_inactive_color_input,
            active_effects_category_input,
            hand_icon_input,
            hand_icon_color_input,
            hand_text_color_input,
            dashboard_layout_input,
            dashboard_spacing_input,
            dashboard_hide_inactive,
            perf_enabled,
            show_desc,
            show_objs,
            show_players,
            show_exits,
            show_name,
            perf_show_fps,
            perf_show_render_times,
            perf_show_ui_times,
            perf_show_wrap_times,
            perf_show_net,
            perf_show_parse,
            perf_show_events,
            perf_show_cpu,
            perf_show_memory,
            perf_show_lines,
            perf_show_uptime,
            perf_show_spike_log,
            perf_show_per_window,
            perf_sparklines,
            available_indicators: Vec::new(),
            perception_sort_direction_input,
            perception_use_short_spell_names,
            show_label_encum: true,
            encum_color_light_input: Self::create_textarea(),
            encum_color_moderate_input: Self::create_textarea(),
            encum_color_heavy_input: Self::create_textarea(),
            encum_color_critical_input: Self::create_textarea(),
            gs4_exp_show_level: true,
            gs4_exp_show_exp_bar: true,
            gs4_exp_show_mind_bar: true,
            gs4_exp_show_total_exp: false,
            gs4_exp_show_ascension_exp: false,
            gs4_exp_mind_bar_color_input: Self::create_textarea(),
            gs4_exp_exp_bar_color_input: Self::create_textarea(),
            minivitals_numbers_only: false,
            minivitals_current_only: false,
            minivitals_health_color_input: Self::create_textarea(),
            minivitals_mana_color_input: Self::create_textarea(),
            minivitals_stamina_color_input: Self::create_textarea(),
            minivitals_spirit_color_input: Self::create_textarea(),
            minivitals_depleted_color_input: Self::create_textarea(),
            betrayer_show_items: true,
            betrayer_bar_color_input: Self::create_textarea(),
            text_compact,
            targets_show_arms_count,
            targets_status_position,
            window_def: window_def.clone(),
            original_window_def: window_def,
            is_new: true,
            status_message: "Tab/Shift+Tab: Navigate | Ctrl+S: Save | Esc: Cancel".to_string(),
            tab_editor: None,
            indicator_editor: None,
            performance_metrics_editor: None,
            text_replacements_editor: None,
            bar_order_editor: None,
            stream_picker: None,
            seen_streams: Vec::new(),
            field_click_areas: Vec::new(),
        }
    }

    /// Create a new window editor with auto-naming for all custom widgets
    /// Uses Layout::generate_spacer_name() for spacers (spacer_N pattern)
    /// Uses Layout::generate_widget_name() for other types (custom-{type}-N pattern)
    pub fn new_window_with_layout(widget_type: String, layout: &crate::config::Layout) -> Self {
        // Prefer the configured template (so defaults like tabs/streams are respected)
        let mut editor = if let Some(template) = crate::core::local_catalog::seed(&widget_type) {
            WindowEditor::new_from_template(template)
        } else {
            WindowEditor::new_window(widget_type.clone())
        };
        editor.available_indicators = Self::indicators_from_layout(layout);

        // Auto-generate a name for all custom widgets
        let auto_name = if widget_type.to_lowercase() == "spacer" {
            // Spacers use the spacer_N pattern for backward compatibility
            layout.generate_spacer_name()
        } else {
            // All other widget types use custom-{type}-N pattern
            layout.generate_widget_name(&widget_type)
        };

        // Set the auto-generated name in both the input field and the window def
        editor.name_input.insert_str(&auto_name);
        editor.window_def.base_mut().name = auto_name;

        editor
    }

}
