//! Keyboard and mouse input routing: text input into the focused field,
//! checkbox toggling, sub-editor key handling, and popup mouse hits.

use super::*;

impl WindowEditor {
    pub fn input(&mut self, input: ratatui::crossterm::event::KeyEvent) {
        // Route input to appropriate TextArea based on focused_field
        let id = self.focused_field;
        match id {
            _ if id == FieldRef::Name.legacy_field_id() => {
                self.name_input.input(input);
            }
            _ if id == FieldRef::Title.legacy_field_id() => {
                self.title_input.input(input);
            }
            _ if id == FieldRef::Row.legacy_field_id() => {
                self.row_input.input(input);
            }
            _ if id == FieldRef::Col.legacy_field_id() => {
                self.col_input.input(input);
            }
            _ if id == FieldRef::Rows.legacy_field_id() => {
                self.rows_input.input(input);
            }
            _ if id == FieldRef::Cols.legacy_field_id() => {
                self.cols_input.input(input);
            }
            _ if id == FieldRef::MinRows.legacy_field_id() => {
                self.min_rows_input.input(input);
            }
            _ if id == FieldRef::MinCols.legacy_field_id() => {
                self.min_cols_input.input(input);
            }
            _ if id == FieldRef::MaxRows.legacy_field_id() => {
                self.max_rows_input.input(input);
            }
            _ if id == FieldRef::MaxCols.legacy_field_id() => {
                self.max_cols_input.input(input);
            }
            _ if id == FieldRef::BgColor.legacy_field_id() => {
                self.bg_color_input.input(input);
            }
            _ if id == FieldRef::BorderColor.legacy_field_id() => {
                self.border_color_input.input(input);
            }
            _ if id == FieldRef::Streams.legacy_field_id() => {
                self.streams_input.input(input);
            }
            _ if id == FieldRef::TextColor.legacy_field_id() => {
                self.text_color_input.input(input);
            }
            _ if id == FieldRef::CursorColor.legacy_field_id() => {
                self.cursor_color_input.input(input);
            }
            _ if id == FieldRef::CursorBg.legacy_field_id() => {
                self.cursor_bg_input.input(input);
            }
            _ if id == FieldRef::CompletionColor.legacy_field_id() => {
                self.completion_color_input.input(input);
            }
            _ if id == FieldRef::ContentAlign.legacy_field_id() => {
                self.content_align_input.input(input);
            }
            _ if id == FieldRef::TabBarPosition.legacy_field_id() => {
                self.tab_bar_position_input.input(input);
            }
            _ if id == FieldRef::TitlePosition.legacy_field_id() => {
                self.title_position_input.input(input);
            }
            _ if id == FieldRef::TabActiveColor.legacy_field_id() => {
                self.tab_active_color_input.input(input);
            }
            _ if id == FieldRef::TabInactiveColor.legacy_field_id() => {
                self.tab_inactive_color_input.input(input);
            }
            _ if id == FieldRef::TabUnreadColor.legacy_field_id() => {
                self.tab_unread_color_input.input(input);
            }
            _ if id == FieldRef::TabUnreadPrefix.legacy_field_id() => {
                self.tab_unread_prefix_input.input(input);
            }
            _ if id == FieldRef::ProgressId.legacy_field_id() => {
                self.progress_id_input.input(input);
            }
            _ if id == FieldRef::ProgressColor.legacy_field_id() => {
                self.progress_color_input.input(input);
            }
            _ if id == FieldRef::CountdownId.legacy_field_id() => {
                self.countdown_id_input.input(input);
            }
            _ if id == FieldRef::CountdownIcon.legacy_field_id() => {
                self.countdown_icon_input.input(input);
            }
            _ if id == FieldRef::CountdownColor.legacy_field_id() => {
                self.countdown_color_input.input(input);
            }
            _ if id == FieldRef::CountdownBgColor.legacy_field_id() => {
                self.countdown_bg_color_input.input(input);
            }
            _ if id == FieldRef::HandIcon.legacy_field_id() => {
                self.hand_icon_input.input(input);
            }
            _ if id == FieldRef::HandIconColor.legacy_field_id() => {
                self.hand_icon_color_input.input(input);
            }
            _ if id == FieldRef::HandTextColor.legacy_field_id() => {
                self.hand_text_color_input.input(input);
            }
            _ if id == FieldRef::CompassActiveColor.legacy_field_id() => {
                self.compass_active_color_input.input(input);
            }
            _ if id == FieldRef::CompassInactiveColor.legacy_field_id() => {
                self.compass_inactive_color_input.input(input);
            }
            _ if id == FieldRef::InjuryDefaultColor.legacy_field_id() => {
                self.injury_default_color_input.input(input);
            }
            _ if id == FieldRef::Injury1Color.legacy_field_id() => {
                self.injury1_color_input.input(input);
            }
            _ if id == FieldRef::Injury2Color.legacy_field_id() => {
                self.injury2_color_input.input(input);
            }
            _ if id == FieldRef::Injury3Color.legacy_field_id() => {
                self.injury3_color_input.input(input);
            }
            _ if id == FieldRef::Scar1Color.legacy_field_id() => {
                self.scar1_color_input.input(input);
            }
            _ if id == FieldRef::Scar2Color.legacy_field_id() => {
                self.scar2_color_input.input(input);
            }
            _ if id == FieldRef::Scar3Color.legacy_field_id() => {
                self.scar3_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsHealthColor.legacy_field_id() => {
                self.minivitals_health_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsManaColor.legacy_field_id() => {
                self.minivitals_mana_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsStaminaColor.legacy_field_id() => {
                self.minivitals_stamina_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsSpiritColor.legacy_field_id() => {
                self.minivitals_spirit_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsDepletedColor.legacy_field_id() => {
                self.minivitals_depleted_color_input.input(input);
            }
            _ if id == FieldRef::EncumColorLight.legacy_field_id() => {
                self.encum_color_light_input.input(input);
            }
            _ if id == FieldRef::EncumColorModerate.legacy_field_id() => {
                self.encum_color_moderate_input.input(input);
            }
            _ if id == FieldRef::EncumColorHeavy.legacy_field_id() => {
                self.encum_color_heavy_input.input(input);
            }
            _ if id == FieldRef::EncumColorCritical.legacy_field_id() => {
                self.encum_color_critical_input.input(input);
            }
            _ if id == FieldRef::GS4ExpMindBarColor.legacy_field_id() => {
                self.gs4_exp_mind_bar_color_input.input(input);
            }
            _ if id == FieldRef::GS4ExpExpBarColor.legacy_field_id() => {
                self.gs4_exp_exp_bar_color_input.input(input);
            }
            _ if id == FieldRef::BetrayerBarColor.legacy_field_id() => {
                self.betrayer_bar_color_input.input(input);
            }
            _ if id == FieldRef::IndicatorId.legacy_field_id() => {
                self.indicator_id_input.input(input);
            }
            _ if id == FieldRef::IndicatorIcon.legacy_field_id() => {
                self.indicator_icon_input.input(input);
            }
            _ if id == FieldRef::IndicatorActiveColor.legacy_field_id() => {
                self.indicator_active_color_input.input(input);
            }
            _ if id == FieldRef::IndicatorInactiveColor.legacy_field_id() => {
                self.indicator_inactive_color_input.input(input);
            }
            _ if id == FieldRef::ActiveEffectsCategory.legacy_field_id() => {
                self.active_effects_category_input.input(input);
            }
            _ if id == FieldRef::PerceptionSortDirection.legacy_field_id() => {
                // Dropdown field - do not accept text input (use Enter/Space to cycle)
            }
            _ if id == FieldRef::PerceptionTextReplacements.legacy_field_id() => {
                // Button field - do not accept text input (use Enter/Space to activate)
            }
            _ if id == FieldRef::PerceptionUseShortSpellNames.legacy_field_id() => {
                // Checkbox field - do not accept text input (use Enter/Space to toggle)
            }
            _ if id == FieldRef::DashboardLayout.legacy_field_id() => {
                // Dropdown field - do not accept text input (use Enter/Space to cycle)
            }
            _ if id == FieldRef::DashboardSpacing.legacy_field_id() => {
                self.dashboard_spacing_input.input(input);
            }
            _ if id == FieldRef::BufferSize.legacy_field_id() => {
                self.buffer_size_input.input(input);
            }
            _ if id == FieldRef::PromptIcon.legacy_field_id() => {
                self.prompt_icon_input.input(input);
            }
            _ if id == FieldRef::PromptIconColor.legacy_field_id() => {
                self.prompt_icon_color_input.input(input);
            }
            _ if id == FieldRef::EntityId.legacy_field_id() => {
                self.entity_id_input.input(input);
            }
            _ => {} // Checkboxes/dropdowns don't handle text input
        }
    }

    pub fn toggle_field(&mut self) {
        match self.focused_field {
            12 => {
                let current = self.window_def.base().show_title;
                self.window_def.base_mut().show_title = !current;
            }
            13 => {
                let current = self.window_def.base().locked;
                self.window_def.base_mut().locked = !current;
            }
            121 => {
                let current = self.window_def.base().tts_speak;
                self.window_def.base_mut().tts_speak = !current;
            }
            14 => {
                let current = self.window_def.base().transparent_background;
                self.window_def.base_mut().transparent_background = !current;
            }
            15 => {
                let new_show = !self.window_def.base().show_border;
                let sides = self.window_def.base().border_sides.clone();
                self.window_def
                    .base_mut()
                    .apply_border_configuration(new_show, sides);
                self.refresh_size_inputs();
            }
            16 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.top = !sides.top;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            17 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.bottom = !sides.bottom;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            18 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.left = !sides.left;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            19 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.right = !sides.right;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            _ => {
                if let Some(field_ref) = self.current_field_ref() {
                    match field_ref {
                        FieldRef::ShowDesc => {
                            self.show_desc = !self.show_desc;
                        }
                        FieldRef::ShowObjs => {
                            self.show_objs = !self.show_objs;
                        }
                        FieldRef::ShowPlayers => {
                            self.show_players = !self.show_players;
                        }
                        FieldRef::ShowExits => {
                            self.show_exits = !self.show_exits;
                        }
                        FieldRef::ShowName => {
                            self.show_name = !self.show_name;
                        }
                        FieldRef::Wordwrap => {
                            self.text_wordwrap = !self.text_wordwrap;
                        }
                        FieldRef::Timestamps => {
                            self.text_show_timestamps = !self.text_show_timestamps;
                        }
                        FieldRef::TextCompact => {
                            self.text_compact = !self.text_compact;
                        }
                        FieldRef::TargetsShowAppendages => {
                            self.targets_show_arms_count = !self.targets_show_arms_count;
                        }
                        FieldRef::TargetsStatusPosition => {
                            // Cycle between "start" and "end"
                            self.targets_status_position = if self.targets_status_position == "start" {
                                "end".to_string()
                            } else {
                                "start".to_string()
                            };
                        }
                        FieldRef::ProgressNumbersOnly => {
                            self.progress_numbers_only = !self.progress_numbers_only;
                        }
                        FieldRef::ProgressCurrentOnly => {
                            self.progress_current_only = !self.progress_current_only;
                        }
                        FieldRef::TabSeparator => {
                            self.tab_separator = !self.tab_separator;
                        }
                        FieldRef::TabBarPosition => {
                            self.cycle_tab_bar_position();
                        }
                        FieldRef::TitlePosition => {
                            self.cycle_title_position(false);
                        }
                        FieldRef::PerceptionSortDirection => {
                            self.cycle_perception_sort_direction();
                        }
                        FieldRef::EditTabs => {
                            self.open_tab_editor();
                        }
                        FieldRef::EditIndicators => {
                            self.open_indicator_editor();
                        }
                        FieldRef::EditMetrics => {
                            self.open_performance_metrics_editor();
                        }
                        FieldRef::PerceptionTextReplacements => {
                            self.open_perception_replacements_editor();
                        }
                        FieldRef::MiniVitalsEditBarOrder => {
                            self.open_bar_order_editor();
                        }
                        FieldRef::DashboardHideInactive => {
                            self.dashboard_hide_inactive = !self.dashboard_hide_inactive;
                        }
                        FieldRef::DashboardLayout => {
                            self.cycle_dashboard_layout();
                        }
                        FieldRef::EncumShowLabel => {
                            let prev_show = self.show_label_encum;
                            self.show_label_encum = !self.show_label_encum;
                            // Apply row adjustment for optional content row
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.show_label_encum, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::GS4ExpShowLevel => {
                            let prev_show = self.gs4_exp_show_level;
                            self.gs4_exp_show_level = !self.gs4_exp_show_level;
                            // Apply row adjustment for optional content row
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.gs4_exp_show_level, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::GS4ExpShowExpBar => {
                            let prev_show = self.gs4_exp_show_exp_bar;
                            self.gs4_exp_show_exp_bar = !self.gs4_exp_show_exp_bar;
                            // Apply row adjustment for optional content row
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.gs4_exp_show_exp_bar, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::GS4ExpShowMindBar => {
                            let prev_show = self.gs4_exp_show_mind_bar;
                            self.gs4_exp_show_mind_bar = !self.gs4_exp_show_mind_bar;
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.gs4_exp_show_mind_bar, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::GS4ExpShowTotalExp => {
                            let prev_show = self.gs4_exp_show_total_exp;
                            self.gs4_exp_show_total_exp = !self.gs4_exp_show_total_exp;
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.gs4_exp_show_total_exp, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::GS4ExpShowAscensionExp => {
                            let prev_show = self.gs4_exp_show_ascension_exp;
                            self.gs4_exp_show_ascension_exp = !self.gs4_exp_show_ascension_exp;
                            self.window_def.base_mut().apply_optional_content_row(
                                self.gs4_exp_show_ascension_exp,
                                prev_show,
                            );
                            self.refresh_size_inputs();
                        }
                        FieldRef::MiniVitalsNumbersOnly => {
                            self.minivitals_numbers_only = !self.minivitals_numbers_only;
                            // If numbers_only is enabled, disable current_only
                            if self.minivitals_numbers_only {
                                self.minivitals_current_only = false;
                            }
                        }
                        FieldRef::MiniVitalsCurrentOnly => {
                            self.minivitals_current_only = !self.minivitals_current_only;
                            // If current_only is enabled, disable numbers_only
                            if self.minivitals_current_only {
                                self.minivitals_numbers_only = false;
                            }
                        }
                        FieldRef::BetrayerShowItems => {
                            let prev_show = self.betrayer_show_items;
                            self.betrayer_show_items = !self.betrayer_show_items;
                            // Adjust rows based on show_items toggle
                            // When toggling OFF: remove item rows (calculate from current - ideal_without_items)
                            // When toggling ON: add 1 row (minimum item row, auto-resize will correct later)
                            // Read min/max from text inputs (not window_def which may not have latest edits)
                            let min_rows: u16 = self
                                .min_rows_input
                                .lines()
                                .first()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(1);
                            let max_rows: u16 = self
                                .max_rows_input
                                .lines()
                                .first()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(u16::MAX);
                            let base = self.window_def.base_mut();
                            let border_rows = base.horizontal_border_units();
                            let bar_rows = 1u16;
                            if prev_show && !self.betrayer_show_items {
                                // Toggling OFF - set rows to just bar + borders
                                let ideal_rows_off = bar_rows + border_rows;
                                base.rows = crate::data::geometry::Height::new(
                                    ideal_rows_off.max(min_rows).min(max_rows),
                                );
                            } else if !prev_show && self.betrayer_show_items {
                                // Toggling ON - add 1 item row (minimum)
                                let ideal_rows_on = bar_rows + 1 + border_rows;
                                base.rows = crate::data::geometry::Height::new(
                                    ideal_rows_on.max(min_rows).min(max_rows),
                                );
                            }
                            self.refresh_size_inputs();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn handle_sub_editor_cancel(&mut self) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            if matches!(editor.mode, TabEditorMode::Form) {
                editor.cancel_form();
                return true;
            }
        }
        if let Some(editor) = self.indicator_editor.as_mut() {
            if matches!(editor.mode, IndicatorEditorMode::Form) {
                editor.cancel_form();
                return true;
            }
        }
        if let Some(editor) = self.text_replacements_editor.as_mut() {
            if matches!(editor.mode, TextReplacementsEditorMode::Form) {
                editor.cancel_form();
                return true;
            }
        }
        self.close_sub_editor()
    }

    pub fn handle_sub_editor_navigation(&mut self, down: bool) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            match editor.mode {
                TabEditorMode::List => {
                    if down {
                        if editor.tabs.is_empty() {
                            editor.selected = 0;
                        } else if editor.selected + 1 < editor.tabs.len() {
                            editor.selected += 1;
                        } else {
                            editor.selected = 0; // wrap
                        }
                    } else if !editor.tabs.is_empty() {
                        if editor.selected == 0 {
                            editor.selected = editor.tabs.len().saturating_sub(1);
                        } else {
                            editor.selected -= 1;
                        }
                    }
                }
                TabEditorMode::Form => {
                    editor.form_field = match (editor.form_field, down) {
                        (TabEditorFormField::Name, true) => TabEditorFormField::Streams,
                        (TabEditorFormField::Streams, true) => TabEditorFormField::Timestamps,
                        (TabEditorFormField::Timestamps, true) => {
                            TabEditorFormField::IgnoreActivity
                        }
                        (TabEditorFormField::IgnoreActivity, true) => TabEditorFormField::Name,
                        (TabEditorFormField::Name, false) => TabEditorFormField::IgnoreActivity,
                        (TabEditorFormField::Streams, false) => TabEditorFormField::Name,
                        (TabEditorFormField::Timestamps, false) => TabEditorFormField::Streams,
                        (TabEditorFormField::IgnoreActivity, false) => {
                            TabEditorFormField::Timestamps
                        }
                    };
                }
            }
            return true;
        }

        if let Some(editor) = self.indicator_editor.as_mut() {
            match editor.mode {
                IndicatorEditorMode::List => {
                    if down {
                        if editor.selected + 1 < editor.indicators.len() {
                            editor.selected += 1;
                        }
                    } else if editor.selected > 0 {
                        editor.selected -= 1;
                    }
                }
                IndicatorEditorMode::Form => {
                    editor.form_field = match (editor.form_field, down) {
                        (IndicatorFormField::Id, true) => IndicatorFormField::Icon,
                        (IndicatorFormField::Icon, true) => IndicatorFormField::Colors,
                        (IndicatorFormField::Colors, true) => IndicatorFormField::Id,
                        (IndicatorFormField::Colors, false) => IndicatorFormField::Icon,
                        (IndicatorFormField::Icon, false) => IndicatorFormField::Id,
                        (IndicatorFormField::Id, false) => IndicatorFormField::Colors,
                    };
                }
            }
            return true;
        }

        if let Some(picker) = self.stream_picker.as_mut() {
            picker.move_selection(down);
            return true;
        }

        if let Some(editor) = self.performance_metrics_editor.as_mut() {
            editor.move_selection(down);
            return true;
        }

        if let Some(editor) = self.text_replacements_editor.as_mut() {
            match editor.mode {
                TextReplacementsEditorMode::List => {
                    let len = editor.replacements.len();
                    if len == 0 {
                        editor.selected = 0;
                    } else if down {
                        editor.selected = (editor.selected + 1) % len;
                    } else if editor.selected == 0 {
                        editor.selected = len.saturating_sub(1);
                    } else {
                        editor.selected -= 1;
                    }
                }
                TextReplacementsEditorMode::Form => {
                    editor.form_field = match (editor.form_field, down) {
                        (TextReplacementsFormField::Pattern, true) => TextReplacementsFormField::Replace,
                        (TextReplacementsFormField::Replace, true) => TextReplacementsFormField::Pattern,
                        (TextReplacementsFormField::Pattern, false) => TextReplacementsFormField::Replace,
                        (TextReplacementsFormField::Replace, false) => TextReplacementsFormField::Pattern,
                    };
                }
            }
            return true;
        }

        if let Some(editor) = self.bar_order_editor.as_mut() {
            if down {
                editor.nav_down();
            } else {
                editor.nav_up();
            }
            return true;
        }

        false
    }

    pub fn handle_sub_editor_reorder(&mut self, down: bool) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            if matches!(editor.mode, TabEditorMode::List) {
                if down {
                    editor.move_down();
                } else {
                    editor.move_up();
                }
                return true;
            }
        }

        if let Some(editor) = self.indicator_editor.as_mut() {
            if matches!(editor.mode, IndicatorEditorMode::List) {
                if down {
                    editor.move_down();
                } else {
                    editor.move_up();
                }
                return true;
            }
        }

        if self.performance_metrics_editor.is_some() {
            return true;
        }

        if let Some(editor) = self.text_replacements_editor.as_mut() {
            if matches!(editor.mode, TextReplacementsEditorMode::List) {
                if down {
                    editor.move_down();
                } else {
                    editor.move_up();
                }
                return true;
            }
        }

        if let Some(editor) = self.bar_order_editor.as_mut() {
            if down {
                editor.move_down();
            } else {
                editor.move_up();
            }
            return true;
        }

        false
    }

    pub fn handle_sub_editor_key(&mut self, key_event: TfKeyEvent) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            match editor.mode {
                TabEditorMode::List => match key_event.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        editor.start_add();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        editor.start_edit();
                        return true;
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                        editor.delete_selected();
                        return true;
                    }
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else if editor.selected > 0 {
                            editor.selected -= 1;
                        } else if !editor.tabs.is_empty() {
                            editor.selected = editor.tabs.len().saturating_sub(1);
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else if editor.selected + 1 < editor.tabs.len() {
                            editor.selected += 1;
                        } else if !editor.tabs.is_empty() {
                            editor.selected = 0;
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                TabEditorMode::Form => match key_event.code {
                    KeyCode::Esc => {
                        editor.cancel_form();
                        return true;
                    }
                    KeyCode::Enter => {
                        editor.save_form();
                        return true;
                    }
                    KeyCode::Tab => {
                        self.handle_sub_editor_navigation(true);
                        return true;
                    }
                    KeyCode::BackTab => {
                        self.handle_sub_editor_navigation(false);
                        return true;
                    }
                    KeyCode::Char(' ') => {
                        match editor.form_field {
                            TabEditorFormField::Timestamps => {
                                editor.show_timestamps = !editor.show_timestamps;
                                return true;
                            }
                            TabEditorFormField::IgnoreActivity => {
                                editor.ignore_activity = !editor.ignore_activity;
                                return true;
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        match editor.form_field {
                            TabEditorFormField::Name => {
                                editor.name_input.input(ev);
                            }
                            TabEditorFormField::Streams => {
                                editor.streams_input.input(ev);
                            }
                            TabEditorFormField::Timestamps => {
                                editor.show_timestamps = !editor.show_timestamps;
                            }
                            TabEditorFormField::IgnoreActivity => {
                                editor.ignore_activity = !editor.ignore_activity;
                            }
                        };
                        return true;
                    }
                },
            }
        }

        if let Some(editor) = self.indicator_editor.as_mut() {
            match editor.mode {
                IndicatorEditorMode::List => match key_event.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        editor.start_add();
                        return true;
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        editor.toggle_selected();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        editor.start_edit();
                        return true;
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                        editor.delete_selected();
                        return true;
                    }
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else if editor.selected > 0 {
                            editor.selected -= 1;
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else if editor.selected + 1 < editor.indicators.len() {
                            editor.selected += 1;
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                IndicatorEditorMode::Form => match key_event.code {
                    KeyCode::Esc => {
                        editor.cancel_form();
                        return true;
                    }
                    KeyCode::Enter => {
                        editor.save_form();
                        return true;
                    }
                    KeyCode::Tab => {
                        self.handle_sub_editor_navigation(true);
                        return true;
                    }
                    KeyCode::BackTab => {
                        self.handle_sub_editor_navigation(false);
                        return true;
                    }
                    _ => {
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        match editor.form_field {
                            IndicatorFormField::Id => {
                                editor.id_input.input(ev);
                            }
                            IndicatorFormField::Icon => {
                                editor.icon_input.input(ev);
                            }
                            IndicatorFormField::Colors => {
                                editor.colors_input.input(ev);
                            }
                        };
                        return true;
                    }
                },
            }
        }

        if self.stream_picker.is_some() {
            match key_event.code {
                KeyCode::Up => {
                    if let Some(picker) = self.stream_picker.as_mut() {
                        picker.move_selection(false);
                    }
                    return true;
                }
                KeyCode::Down => {
                    if let Some(picker) = self.stream_picker.as_mut() {
                        picker.move_selection(true);
                    }
                    return true;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    // Append the highlighted id; keep the picker open so several
                    // streams can be added in a row.
                    let id = self
                        .stream_picker
                        .as_ref()
                        .and_then(|picker| picker.selected_id())
                        .map(str::to_string);
                    if let Some(id) = id {
                        self.append_stream_to_field(&id);
                    }
                    return true;
                }
                KeyCode::Esc => {
                    self.close_sub_editor();
                    return true;
                }
                _ => {}
            }
        }

        if let Some(editor) = self.performance_metrics_editor.as_mut() {
            match key_event.code {
                KeyCode::Up => {
                    editor.move_selection(false);
                    return true;
                }
                KeyCode::Down => {
                    editor.move_selection(true);
                    return true;
                }
                KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Char(' ') | KeyCode::Enter => {
                    editor.toggle_selected();
                    return true;
                }
                KeyCode::Esc => {
                    self.close_sub_editor();
                    return true;
                }
                _ => {}
            }
        }

        if let Some(editor) = self.text_replacements_editor.as_mut() {
            match editor.mode {
                TextReplacementsEditorMode::List => match key_event.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        editor.start_add();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        if !editor.replacements.is_empty() {
                            editor.start_edit();
                        }
                        return true;
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                        editor.delete_selected();
                        return true;
                    }
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else if editor.replacements.is_empty() {
                            editor.selected = 0;
                        } else if editor.selected == 0 {
                            editor.selected = editor.replacements.len().saturating_sub(1);
                        } else {
                            editor.selected -= 1;
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else if editor.replacements.is_empty() {
                            editor.selected = 0;
                        } else {
                            editor.selected = (editor.selected + 1) % editor.replacements.len();
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                TextReplacementsEditorMode::Form => match key_event.code {
                    KeyCode::Esc => {
                        editor.cancel_form();
                        return true;
                    }
                    KeyCode::Enter => {
                        editor.save_form();
                        return true;
                    }
                    KeyCode::Tab => {
                        self.handle_sub_editor_navigation(true);
                        return true;
                    }
                    KeyCode::BackTab => {
                        self.handle_sub_editor_navigation(false);
                        return true;
                    }
                    _ => {
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        match editor.form_field {
                            TextReplacementsFormField::Pattern => {
                                editor.pattern_input.input(ev);
                            }
                            TextReplacementsFormField::Replace => {
                                editor.replace_input.input(ev);
                            }
                        };
                        return true;
                    }
                },
            }
        }

        if let Some(editor) = self.bar_order_editor.as_mut() {
            match editor.focus {
                BarOrderEditorFocus::Toggle => match key_event.code {
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else {
                            editor.nav_up();
                            editor.sync_color_input_from_bar();
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else {
                            editor.nav_down();
                            editor.sync_color_input_from_bar();
                        }
                        return true;
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Char(' ') | KeyCode::Enter => {
                        if !editor.toggle_selected() {
                            self.status_message = format!(
                                "Max {} bars enabled. Disable one first.",
                                BarOrderEditor::MAX_ENABLED
                            );
                        }
                        return true;
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        // T→C: Move to color column (same row)
                        editor.focus_color();
                        return true;
                    }
                    KeyCode::BackTab | KeyCode::Left => {
                        // Go to previous row's color (wrap to last if at first)
                        if editor.selected > 0 {
                            editor.selected -= 1;
                        } else {
                            editor.selected = editor.bars.len().saturating_sub(1);
                        }
                        editor.sync_color_input_from_bar();
                        editor.focus_color();
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                BarOrderEditorFocus::Color => match key_event.code {
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        // C→T: Save color, move to next row's toggle (wrap to first if at last)
                        editor.save_color_to_bar();
                        if editor.selected + 1 < editor.bars.len() {
                            editor.selected += 1;
                        } else {
                            editor.selected = 0;
                        }
                        editor.sync_color_input_from_bar();
                        editor.focus_toggle();
                        return true;
                    }
                    KeyCode::BackTab | KeyCode::Left => {
                        // C→T: Save and move back to same row's toggle
                        editor.focus_toggle();
                        return true;
                    }
                    KeyCode::Up => {
                        // Save current, move to previous bar's color
                        editor.save_color_to_bar();
                        editor.nav_up();
                        editor.sync_color_input_from_bar();
                        return true;
                    }
                    KeyCode::Down | KeyCode::Enter => {
                        // Save current, move to next bar's color
                        editor.save_color_to_bar();
                        editor.nav_down();
                        editor.sync_color_input_from_bar();
                        return true;
                    }
                    _ => {
                        // Forward keypress to color input textarea
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        editor.color_input.input(ev);
                        return true;
                    }
                },
            }
        }

        false
    }

    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> WindowEditorMouseAction {
        if !mouse_down {
            self.dragging = false;
            return WindowEditorMouseAction::None;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: self.popup_width,
            height: self.popup_height,
        };

        // Check if mouse is on the title bar (for dragging)
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > popup_area.x
            && mouse_col < popup_area.x + popup_area.width.saturating_sub(1);

        // Start drag if on title bar
        if on_title_bar && !self.dragging {
            self.dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
        }

        // Handle dragging
        if self.dragging {
            self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
            self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
            self.popup_x = self.popup_x.min(area.width.saturating_sub(self.popup_width));
            self.popup_y = self.popup_y.min(area.height.saturating_sub(self.popup_height));
            return WindowEditorMouseAction::None; // Don't process field clicks while dragging
        }

        // Check if click is inside the popup (but not on title bar)
        let inside_popup = mouse_col >= popup_area.x
            && mouse_col < popup_area.x + popup_area.width
            && mouse_row > popup_area.y  // Skip title bar row
            && mouse_row < popup_area.y + popup_area.height;

        if !inside_popup {
            return WindowEditorMouseAction::None;
        }

        // Check if click is on the footer (bottom border with Save/Cancel)
        let footer_y = popup_area.y + popup_area.height.saturating_sub(1);
        if mouse_row == footer_y {
            // Footer text: "[Ctrl+S: Save] [Esc: Cancel]"
            // Check relative x position within the popup
            let rel_x = mouse_col.saturating_sub(popup_area.x);

            // Save is roughly in the left portion, Cancel in the right
            // "[Ctrl+S: Save]" starts around x=1, "Save" is around x=9-12
            // "[Esc: Cancel]" starts around x=16, "Cancel" is around x=22-27
            if rel_x >= 1 && rel_x <= 14 {
                return WindowEditorMouseAction::Save;
            } else if rel_x >= 16 && rel_x <= 28 {
                return WindowEditorMouseAction::Cancel;
            }
        }

        // Handle bar order editor clicks
        if let Some(ref mut editor) = self.bar_order_editor {
            if editor.handle_mouse_click(mouse_col, mouse_row) {
                return WindowEditorMouseAction::None;
            }
        }

        // Handle tab editor clicks
        if let Some(ref mut editor) = self.tab_editor {
            if editor.handle_mouse_click(mouse_col, mouse_row) {
                return WindowEditorMouseAction::None;
            }
        }

        // Handle indicator editor clicks
        if let Some(ref mut editor) = self.indicator_editor {
            if editor.handle_mouse_click(mouse_col, mouse_row) {
                return WindowEditorMouseAction::None;
            }
        }

        // Check if click is on a tracked field area
        // field_click_areas contains (y, x_start, field_ref)
        // We match by y (exact row) and distinguish side-by-side fields by x
        let left_column_end = self.popup_x + 37;  // Left column ends at x=37 relative to popup
        let geom_x2 = self.popup_x + 17;  // Divider for side-by-side geometry fields (Row/Col, etc.)

        // Find all fields on this row
        let fields_on_row: Vec<_> = self.field_click_areas.iter()
            .filter(|(y, _, _)| *y == mouse_row)
            .collect();

        for &(field_y, field_x, ref field_ref) in &self.field_click_areas {
            if mouse_row == field_y {
                // Check if there are multiple fields on this row (side-by-side)
                let multiple_on_row = fields_on_row.len() > 1;

                let in_field_region = if field_x >= left_column_end {
                    // Right column field: accept clicks in right half
                    mouse_col >= left_column_end
                } else if multiple_on_row && field_x < left_column_end {
                    // Left column with side-by-side fields (Row/Col, Rows/Cols, etc.)
                    // First field is at left_x, second at geom_x2
                    if field_x < geom_x2 {
                        // First field (Row, Rows, Min, Max): accept clicks up to geom_x2
                        mouse_col >= self.popup_x && mouse_col < geom_x2
                    } else {
                        // Second field (Col, Cols): accept clicks from geom_x2 onwards in left column
                        mouse_col >= geom_x2 && mouse_col < left_column_end
                    }
                } else {
                    // Single field in left column: accept clicks in left half
                    mouse_col >= self.popup_x && mouse_col < left_column_end
                };

                if in_field_region {
                    // Update focused field
                    self.focused_field = field_ref.legacy_field_id();
                    // Also update current_field_index to match
                    if let Some(idx) = self.field_order.iter().position(|f| f == field_ref) {
                        self.current_field_index = idx;
                    }

                    // Toggle checkboxes when clicked
                    if self.is_on_checkbox() {
                        self.toggle_field();
                    } else if self.is_on_content_align() {
                        self.cycle_content_align(false);
                    } else if self.is_on_title_position() {
                        self.cycle_title_position(false);
                    } else if self.is_on_border_style() {
                        self.cycle_border_style(false);
                    } else if self.is_on_dashboard_layout() {
                        self.cycle_dashboard_layout();
                    } else if self.is_on_edit_indicators() {
                        self.open_indicator_editor();
                    } else if self.is_on_edit_tabs() {
                        self.open_tab_editor();
                    } else if self.is_on_edit_bar_order() {
                        self.open_bar_order_editor();
                    } else if self.is_on_perception_sort_direction() {
                        self.cycle_perception_sort_direction();
                    } else if self.is_on_perception_replacements() {
                        self.open_perception_replacements_editor();
                    } else if self.is_on_tab_bar_position() {
                        self.cycle_tab_bar_position();
                    }

                    return WindowEditorMouseAction::None;
                }
            }
        }

        WindowEditorMouseAction::None
    }

}
