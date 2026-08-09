//! Syncing editor field values back onto the WindowDef, plus save-time
//! validation and the editor lifecycle accessors.

use super::*;

impl WindowEditor {
    pub fn sync_to_window_def(&mut self) {
        self.commit_sub_editors();
        self.window_def.base_mut().name = self.name_input.lines()[0].to_string();
        self.window_def.base_mut().title =
            Some(self.title_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        self.window_def.base_mut().row =
            crate::data::geometry::Row::new(self.row_input.lines()[0].parse().unwrap_or(0));
        self.window_def.base_mut().col =
            crate::data::geometry::Col::new(self.col_input.lines()[0].parse().unwrap_or(0));
        // Rows/cols is now total size (VellumFE style), not content size
        // User specifies actual widget dimensions; content adjusts based on borders
        let total_rows = self.rows_input.lines().first()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(1);
        let total_cols = self.cols_input.lines().first()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(40);
        self.window_def.base_mut().rows = crate::data::geometry::Height::new(total_rows.max(1));
        self.window_def.base_mut().cols = crate::data::geometry::Width::new(total_cols.max(1));
        self.window_def.base_mut().min_rows = self.min_rows_input.lines()[0].parse().ok();
        self.window_def.base_mut().min_cols = self.min_cols_input.lines()[0].parse().ok();
        self.window_def.base_mut().max_rows = self.max_rows_input.lines()[0].parse().ok();
        self.window_def.base_mut().max_cols = self.max_cols_input.lines()[0].parse().ok();
        self.window_def.base_mut().background_color =
            Some(self.bg_color_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        self.window_def.base_mut().border_color =
            Some(self.border_color_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        if matches!(self.window_def, crate::config::WindowDef::Progress { .. }) {
            self.window_def.base_mut().text_color =
                Some(self.text_color_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        }
        self.window_def.base_mut().title_position = self
            .title_position_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "top-left".to_string());
        self.window_def.base_mut().content_align =
            Some(self.content_align_input.lines()[0].to_string()).filter(|s| !s.is_empty());

        // Update streams only for Text variant
        if let crate::config::WindowDef::Text { data, .. } = &mut self.window_def {
            let streams: Vec<String> = self.streams_input.lines()[0]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            data.streams = streams;
            data.buffer_size = self
                .buffer_size_input
                .lines()
                .get(0)
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(data.buffer_size);
            data.wordwrap = self.text_wordwrap;
            data.show_timestamps = self.text_show_timestamps;
            data.compact = self.text_compact;
        }

        if let crate::config::WindowDef::Inventory { data, .. }
        | crate::config::WindowDef::Reserve { data, .. } = &mut self.window_def
        {
            let streams: Vec<String> = self.streams_input.lines()[0]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            data.streams = streams;
            data.buffer_size = self
                .buffer_size_input
                .lines()
                .get(0)
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(data.buffer_size);
            data.wordwrap = self.text_wordwrap;
            data.show_timestamps = self.text_show_timestamps;
        }

        if let crate::config::WindowDef::TabbedText { data, .. } = &mut self.window_def {
            data.tab_bar_position = self
                .tab_bar_position_input
                .lines()
                .get(0)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "top".to_string());
            data.tab_active_color = self
                .tab_active_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_inactive_color = self
                .tab_inactive_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_unread_color = self
                .tab_unread_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_unread_prefix = self
                .tab_unread_prefix_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_separator = self.tab_separator;
        }

        if let crate::config::WindowDef::Room { data, .. } = &mut self.window_def {
            data.show_desc = self.show_desc;
            data.show_objs = self.show_objs;
            data.show_players = self.show_players;
            data.show_exits = self.show_exits;
            data.show_name = self.show_name;
        }

        if let crate::config::WindowDef::Perception { data, .. } = &mut self.window_def {
            // Stream is ALWAYS "percWindow" - hardcoded, not user-editable
            data.stream = "percWindow".to_string();

            // Buffer size is ALWAYS 100 - hardcoded (window clears on each update)
            data.buffer_size = 100;

            // Parse sort direction
            data.sort_direction = match self.perception_sort_direction_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_lowercase())
                .as_deref()
            {
                Some("ascending") => crate::config::SortDirection::Ascending,
                _ => crate::config::SortDirection::Descending,
            };

            // Short spell names toggle
            data.use_short_spell_names = self.perception_use_short_spell_names;

            // text_replacements are handled by the TextReplacementsEditor
        }

        if let crate::config::WindowDef::Progress { data, .. } = &mut self.window_def {
            data.id = self
                .progress_id_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color = self
                .progress_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.numbers_only = self.progress_numbers_only;
            data.current_only = self.progress_current_only;
        }

        if let crate::config::WindowDef::Encumbrance { data, .. } = &mut self.window_def {
            data.show_label = self.show_label_encum;
            data.color_light = self.encum_color_light_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color_moderate = self.encum_color_moderate_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color_heavy = self.encum_color_heavy_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color_critical = self.encum_color_critical_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::GS4Experience { data, .. } = &mut self.window_def {
            data.show_level = self.gs4_exp_show_level;
            data.show_exp_bar = self.gs4_exp_show_exp_bar;
            data.show_mind_bar = self.gs4_exp_show_mind_bar;
            data.show_total_exp = self.gs4_exp_show_total_exp;
            data.show_ascension_exp = self.gs4_exp_show_ascension_exp;
            data.mind_bar_color = self.gs4_exp_mind_bar_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.exp_bar_color = self.gs4_exp_exp_bar_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::MiniVitals { data, .. } = &mut self.window_def {
            data.numbers_only = self.minivitals_numbers_only;
            data.current_only = self.minivitals_current_only;
            data.health_color = self.minivitals_health_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.mana_color = self.minivitals_mana_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.stamina_color = self.minivitals_stamina_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.spirit_color = self.minivitals_spirit_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.depleted_color = self.minivitals_depleted_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Betrayer { data, .. } = &mut self.window_def {
            data.show_items = self.betrayer_show_items;
            data.bar_color = self.betrayer_bar_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Countdown { data, .. } = &mut self.window_def {
            data.id = self
                .countdown_id_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.icon = self
                .countdown_icon_input
                .lines()
                .get(0)
                .and_then(|s| s.chars().next());
            data.color = self
                .countdown_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.countdown_background_color = self
                .countdown_bg_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Hand { data, .. } = &mut self.window_def {
            data.icon = self
                .hand_icon_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.icon_color = self
                .hand_icon_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.hand_text_color = self
                .hand_text_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Compass { data, .. } = &mut self.window_def {
            data.active_color = self
                .compass_active_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.inactive_color = self
                .compass_inactive_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::InjuryDoll { data, .. } = &mut self.window_def {
            data.injury_default_color = self
                .injury_default_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.injury1_color = self
                .injury1_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.injury2_color = self
                .injury2_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.injury3_color = self
                .injury3_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.scar1_color = self
                .scar1_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.scar2_color = self
                .scar2_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.scar3_color = self
                .scar3_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Indicator { data, .. } = &mut self.window_def {
            data.indicator_id = self
                .indicator_id_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.icon = self
                .indicator_icon_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.active_color = self
                .indicator_active_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.inactive_color = self
                .indicator_inactive_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Dashboard { data, .. } = &mut self.window_def {
            data.layout = self
                .dashboard_layout_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "horizontal".to_string());
            data.spacing = self
                .dashboard_spacing_input
                .lines()
                .get(0)
                .and_then(|s| s.trim().parse::<u16>().ok())
                .unwrap_or(1);
            data.hide_inactive = self.dashboard_hide_inactive;
        }

        if let crate::config::WindowDef::ActiveEffects { data, .. } = &mut self.window_def {
            data.category = self
                .active_effects_category_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "ActiveSpells".to_string());
        }

        if let crate::config::WindowDef::Performance { data, .. } = &mut self.window_def {
            data.enabled = self.perf_enabled;
            data.show_fps = self.perf_show_fps;
            data.show_render_times = self.perf_show_render_times;
            data.show_ui_times = self.perf_show_ui_times;
            data.show_wrap_times = self.perf_show_wrap_times;
            data.show_net = self.perf_show_net;
            data.show_parse = self.perf_show_parse;
            data.show_events = self.perf_show_events;
            data.show_cpu = self.perf_show_cpu;
            data.show_memory = self.perf_show_memory;
            data.show_lines = self.perf_show_lines;
            data.show_uptime = self.perf_show_uptime;
            data.show_spike_log = self.perf_show_spike_log;
            data.show_per_window = self.perf_show_per_window;
            data.sparklines = self.perf_sparklines;
        }

        if let crate::config::WindowDef::CommandInput { data, .. } = &mut self.window_def {
            data.prompt_icon = Some(self.prompt_icon_input.lines()[0].trim().to_string())
                .filter(|s| !s.is_empty());
            data.prompt_icon_color =
                Some(self.prompt_icon_color_input.lines()[0].trim().to_string())
                    .filter(|s| !s.is_empty());
            data.input_text_color =
                Some(self.text_color_input.lines()[0].trim().to_string()).filter(|s| !s.is_empty());
            data.cursor_color = Some(self.cursor_color_input.lines()[0].trim().to_string())
                .filter(|s| !s.is_empty());
            data.cursor_background_color =
                Some(self.cursor_bg_input.lines()[0].trim().to_string()).filter(|s| !s.is_empty());
            data.completion_color =
                Some(self.completion_color_input.lines()[0].trim().to_string())
                    .filter(|s| !s.is_empty());
        }
        if let crate::config::WindowDef::Targets { data, .. } = &mut self.window_def {
            data.entity_id = self.entity_id_input.lines()[0].trim().to_string();
            data.show_body_part_count = self.targets_show_arms_count;
            // Save status_position (None = use global config)
            data.status_position = if self.targets_status_position == "end" {
                None // Default, don't need to save
            } else {
                Some(self.targets_status_position.clone())
            };
        }
        if let crate::config::WindowDef::Players { data, .. } = &mut self.window_def {
            data.entity_id = self.entity_id_input.lines()[0].trim().to_string();
        }
    }

    pub fn get_window_def(&mut self) -> &WindowDef {
        self.sync_to_window_def();
        &self.window_def
    }

    /// Validate before saving (name required, no duplicates when creating/renaming).
    pub fn validate_before_save(&mut self, layout: &crate::config::Layout) -> bool {
        self.sync_to_window_def();

        // Trim the name and write it back to the model
        let trimmed = self.window_def.name().trim().to_string();
        self.window_def.base_mut().name = trimmed.clone();

        if trimmed.is_empty() {
            self.status_message = "Name is required to save".to_string();
            return false;
        }

        // If this is a new window or the name changed, ensure uniqueness
        let original_name = self.original_window_def.name();
        if self.is_new || !trimmed.eq_ignore_ascii_case(original_name) {
            if layout
                .windows
                .iter()
                .any(|w| w.name().eq_ignore_ascii_case(&trimmed))
            {
                self.status_message = format!("Name '{}' is already in use", trimmed);
                return false;
            }
        }

        // Clear any previous warning
        self.status_message = "Tab/Shift+Tab: Navigate | Ctrl+S: Save | Esc: Cancel".to_string();
        true
    }

    pub fn is_new(&self) -> bool {
        self.is_new
    }

    /// The name of the template/window the editor was created from
    pub fn original_name(&self) -> &str {
        self.original_window_def.name()
    }

    pub fn cancel(&mut self) {
        self.window_def = self.original_window_def.clone();
    }

    /// Get the current editor window position and size for persistence
    pub fn get_editor_geometry(&self) -> (u16, u16, u16, u16) {
        (self.popup_x, self.popup_y, self.popup_width, self.popup_height)
    }

}
