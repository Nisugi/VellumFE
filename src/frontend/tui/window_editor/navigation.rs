//! Field navigation and value cycling: moving between fields, the
//! is_on_* predicates the input layer keys off, and cycle_* for dropdown
//! style fields, plus opening/committing the modal sub-editors.

use super::*;

impl WindowEditor {
    pub(super) fn current_field_ref(&self) -> Option<FieldRef> {
        self.field_order.get(self.current_field_index).copied()
    }

    /// Move to next field (Tab)
    pub fn next_field(&mut self) {
        if self.field_order.is_empty() {
            return;
        }

        self.current_field_index = (self.current_field_index + 1) % self.field_order.len();
        self.sync_focused_field();
    }

    /// Move to previous field (Shift+Tab)
    pub fn previous_field(&mut self) {
        if self.field_order.is_empty() {
            return;
        }

        self.current_field_index = if self.current_field_index == 0 {
            self.field_order.len() - 1
        } else {
            self.current_field_index - 1
        };

        self.sync_focused_field();
    }

    /// Sync the legacy focused_field index with current global field
    pub(super) fn sync_focused_field(&mut self) {
        if let Some(field_ref) = self.current_field_ref() {
            self.focused_field = field_ref.legacy_field_id();
        }
    }

    pub fn is_sub_editor_active(&self) -> bool {
        self.tab_editor.is_some()
            || self.indicator_editor.is_some()
            || self.performance_metrics_editor.is_some()
            || self.text_replacements_editor.is_some()
            || self.bar_order_editor.is_some()
            || self.stream_picker.is_some()
    }

    pub(super) fn footer_help_text(&self) -> &str {
        if self.stream_picker.is_some() {
            return "[Enter: Add stream]─[Esc: Back]";
        }
        if self.performance_metrics_editor.is_some() {
            return "[Space/Enter/T: Toggle]─[Esc: Back]";
        }
        if let Some(editor) = self.indicator_editor.as_ref() {
            if matches!(editor.mode, IndicatorEditorMode::List) {
                return "[A: Add]─[E: Edit]─[T: Toggle]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]";
            }
        }
        if let Some(editor) = self.tab_editor.as_ref() {
            if matches!(editor.mode, TabEditorMode::List) {
                return "[A: Add]─[E: Edit]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]";
            }
        }
        if let Some(editor) = self.text_replacements_editor.as_ref() {
            if matches!(editor.mode, TextReplacementsEditorMode::List) {
                return "[A: Add]─[E: Edit]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]";
            }
        }
        if self.bar_order_editor.is_some() {
            return "[Ctrl+S: Save]─[Shift+↑/↓: Reorder]─[Esc: Cancel]";
        }
        "[Ctrl+S: Save] [Esc: Cancel]"
    }

    pub(super) fn open_tab_editor(&mut self) {
        if let WindowDef::TabbedText { data, .. } = &self.window_def {
            self.tab_editor = Some(TabEditor::from_tabs(&data.tabs));
        } else {
            self.status_message = "Tab editor only available for TabbedText windows".to_string();
        }
    }

    pub(super) fn open_indicator_editor(&mut self) {
        if self.available_indicators.is_empty() {
            self.available_indicators = Self::indicator_templates();
        }
        if let WindowDef::Dashboard { data, .. } = &self.window_def {
            self.indicator_editor = Some(IndicatorEditor::from_defs(
                &data.indicators,
                self.available_indicators.clone(),
            ));
        } else {
            self.status_message =
                "Indicator editor only available for Dashboard windows".to_string();
        }
    }

    pub(super) fn open_performance_metrics_editor(&mut self) {
        let items = self.perf_group_states();
        self.performance_metrics_editor = Some(PerformanceMetricsEditor::new(items));
    }

    /// Seed the streams-seen-this-session snapshot. Called by the input layer,
    /// which has AppCore in scope, right after the editor is constructed.
    pub fn set_seen_streams(&mut self, seen: Vec<(String, Option<String>)>) {
        self.seen_streams = seen;
    }

    /// Open the seen-streams picker over the current snapshot. Does nothing if
    /// no streams have been observed yet (the caller surfaces a status message).
    pub fn open_stream_picker(&mut self) -> bool {
        if self.seen_streams.is_empty() {
            self.status_message = "No custom streams seen yet this session.".to_string();
            return false;
        }
        self.stream_picker = Some(StreamPicker::new(self.seen_streams.clone()));
        true
    }

    /// Replace the Streams field wholesale. Used by the `.streams` menu's
    /// "New window on this stream" flow so template default streams don't
    /// linger next to the pre-filled id.
    pub fn set_streams_field(&mut self, streams: &str) {
        self.streams_input = Self::create_textarea();
        self.streams_input.insert_str(streams);
    }

    /// Append a stream id to the Streams field's comma-separated list, skipping
    /// duplicates (case-insensitive). Mirrors the GUI `append_stream_id` helper.
    pub(super) fn append_stream_to_field(&mut self, id: &str) {
        let current = self
            .streams_input
            .lines()
            .first()
            .cloned()
            .unwrap_or_default();
        let already = current
            .split(',')
            .any(|s| s.trim().eq_ignore_ascii_case(id));
        if already {
            return;
        }
        let trimmed = current.trim_end().trim_end_matches(',');
        let next = if trimmed.is_empty() {
            id.to_string()
        } else {
            format!("{}, {}", trimmed, id)
        };
        // Replace the single-line TextArea contents wholesale.
        self.streams_input = Self::create_textarea();
        self.streams_input.insert_str(&next);
    }

    pub(super) fn open_perception_replacements_editor(&mut self) {
        if let WindowDef::Perception { data, .. } = &self.window_def {
            self.text_replacements_editor = Some(TextReplacementsEditor::from_replacements(
                &data.text_replacements,
            ));
        } else {
            self.status_message =
                "Text replacements editor only available for Perception windows".to_string();
        }
    }

    pub(super) fn open_bar_order_editor(&mut self) {
        if let WindowDef::MiniVitals { data, .. } = &self.window_def {
            self.bar_order_editor = Some(BarOrderEditor::from_minivitals_data(data));
        } else {
            self.status_message =
                "Bar order editor only available for MiniVitals windows".to_string();
        }
    }

    pub(super) fn commit_bar_order_editor(&mut self) {
        // Save any pending color input before committing
        if let Some(ref mut editor) = self.bar_order_editor {
            if editor.is_editing_color() {
                editor.save_color_to_bar();
            }
        }
        if let (Some(editor), WindowDef::MiniVitals { data, .. }) =
            (self.bar_order_editor.clone(), &mut self.window_def)
        {
            data.bar_order = editor.to_bar_order();
            editor.apply_colors_to_data(data);
        }
    }

    pub(super) fn commit_text_replacements_editor(&mut self) {
        if let (Some(editor), WindowDef::Perception { data, .. }) =
            (self.text_replacements_editor.clone(), &mut self.window_def)
        {
            // If the editor is in form mode, capture in-progress edits
            let mut editor = editor;
            if editor.mode == TextReplacementsEditorMode::Form {
                editor.save_form();
            }
            data.text_replacements = editor.to_replacements();
            self.text_replacements_editor = Some(editor);
        }
    }

    pub(super) fn commit_tab_editor(&mut self) {
        if let (Some(tab_editor), WindowDef::TabbedText { data, .. }) =
            (self.tab_editor.clone(), &mut self.window_def)
        {
            // If the tab editor is currently in form mode, capture the in-progress edits
            let mut editor = tab_editor;
            if editor.mode == TabEditorMode::Form {
                // save_form will no-op if the inputs are empty
                editor.save_form();
            }
            data.tabs = editor.to_tabs();
            // Update the in-memory editor so subsequent interactions reflect saved values
            self.tab_editor = Some(editor);
        }
    }

    pub(super) fn commit_indicator_editor(&mut self) {
        if let (Some(editor), WindowDef::Dashboard { data, .. }) =
            (&self.indicator_editor, &mut self.window_def)
        {
            data.indicators = editor.to_defs();
        }
    }

    pub(super) fn commit_performance_metrics_editor(&mut self) {
        if let Some(editor) = &self.performance_metrics_editor {
            let items = editor.items.clone();
            self.apply_perf_group_states(&items);
        }
    }

    pub fn commit_sub_editors(&mut self) {
        if self.tab_editor.is_some() {
            self.commit_tab_editor();
        }
        if self.indicator_editor.is_some() {
            self.commit_indicator_editor();
        }
        if self.performance_metrics_editor.is_some() {
            self.commit_performance_metrics_editor();
        }
        if self.text_replacements_editor.is_some() {
            self.commit_text_replacements_editor();
        }
        if self.bar_order_editor.is_some() {
            self.commit_bar_order_editor();
        }
    }

    pub(super) fn perf_group_states(&self) -> Vec<PerfMetricGroupState> {
        vec![
            PerfMetricGroupState {
                group: PerfMetricGroup::FrameTiming,
                enabled: self.perf_show_fps,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::RenderPipeline,
                enabled: self.perf_show_render_times
                    || self.perf_show_ui_times
                    || self.perf_show_wrap_times,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Network,
                enabled: self.perf_show_net,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Parser,
                enabled: self.perf_show_parse,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Events,
                enabled: self.perf_show_events,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Memory,
                enabled: self.perf_show_cpu || self.perf_show_memory,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::UptimeLines,
                enabled: self.perf_show_uptime || self.perf_show_lines,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Diagnostics,
                enabled: self.perf_show_spike_log || self.perf_show_per_window,
            },
        ]
    }

    pub(super) fn apply_perf_group_states(&mut self, states: &[PerfMetricGroupState]) {
        for state in states {
            match state.group {
                PerfMetricGroup::FrameTiming => {
                    self.perf_show_fps = state.enabled;
                }
                PerfMetricGroup::RenderPipeline => {
                    self.perf_show_render_times = state.enabled;
                    self.perf_show_ui_times = state.enabled;
                    self.perf_show_wrap_times = state.enabled;
                }
                PerfMetricGroup::Network => {
                    self.perf_show_net = state.enabled;
                }
                PerfMetricGroup::Parser => {
                    self.perf_show_parse = state.enabled;
                }
                PerfMetricGroup::Events => {
                    self.perf_show_events = state.enabled;
                }
                PerfMetricGroup::Memory => {
                    self.perf_show_cpu = state.enabled;
                    self.perf_show_memory = state.enabled;
                }
                PerfMetricGroup::UptimeLines => {
                    self.perf_show_uptime = state.enabled;
                    self.perf_show_lines = state.enabled;
                }
                PerfMetricGroup::Diagnostics => {
                    self.perf_show_spike_log = state.enabled;
                    self.perf_show_per_window = state.enabled;
                }
            }
        }
    }

    /// Save the active sub-editor form (tab/indicator) and keep the editor open.
    /// Returns true if a sub-editor form was active and handled.
    pub fn save_active_sub_editor_form(&mut self) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            if matches!(editor.mode, TabEditorMode::Form) {
                editor.save_form();
                return true;
            }
        }
        if let Some(editor) = self.indicator_editor.as_mut() {
            if matches!(editor.mode, IndicatorEditorMode::Form) {
                editor.save_form();
                return true;
            }
        }
        if let Some(editor) = self.text_replacements_editor.as_mut() {
            if matches!(editor.mode, TextReplacementsEditorMode::Form) {
                editor.save_form();
                return true;
            }
        }
        false
    }

    pub(super) fn close_sub_editor(&mut self) -> bool {
        if self.stream_picker.is_some() {
            // Selections are applied immediately on Enter; nothing to commit.
            self.stream_picker = None;
            return true;
        }
        if self.tab_editor.is_some() {
            self.commit_tab_editor();
            self.tab_editor = None;
            return true;
        }
        if self.indicator_editor.is_some() {
            self.commit_indicator_editor();
            self.indicator_editor = None;
            return true;
        }
        if self.performance_metrics_editor.is_some() {
            self.commit_performance_metrics_editor();
            self.performance_metrics_editor = None;
            return true;
        }
        if self.text_replacements_editor.is_some() {
            self.commit_text_replacements_editor();
            self.text_replacements_editor = None;
            return true;
        }
        if self.bar_order_editor.is_some() {
            self.commit_bar_order_editor();
            self.bar_order_editor = None;
            return true;
        }
        false
    }

    /// Tab navigation (calls next_field for compatibility)
    pub fn navigate_down(&mut self) {
        self.next_field();
    }

    /// Up arrow navigation (calls previous_field for compatibility)
    pub fn navigate_up(&mut self) {
        self.previous_field();
    }

    /// Check if the currently focused field is a checkbox (fields 12-19)
    pub fn is_on_checkbox(&self) -> bool {
        matches!(
            self.current_field_ref(),
            Some(
                FieldRef::ShowTitle
                    | FieldRef::Locked
                    | FieldRef::TtsSpeak
                    | FieldRef::TransparentBg
                    | FieldRef::ShowBorder
                    | FieldRef::BorderTop
                    | FieldRef::BorderBottom
                    | FieldRef::BorderLeft
                    | FieldRef::BorderRight
                    | FieldRef::ShowDesc
                    | FieldRef::ShowObjs
                    | FieldRef::ShowPlayers
                    | FieldRef::ShowExits
                    | FieldRef::ShowName
                    | FieldRef::Wordwrap
                    | FieldRef::Timestamps
                    | FieldRef::ProgressNumbersOnly
                    | FieldRef::ProgressCurrentOnly
                    | FieldRef::TabSeparator
                    | FieldRef::DashboardHideInactive
                    | FieldRef::EncumShowLabel
                    | FieldRef::GS4ExpShowLevel
                    | FieldRef::GS4ExpShowExpBar
                    | FieldRef::GS4ExpShowMindBar
                    | FieldRef::GS4ExpShowTotalExp
                    | FieldRef::GS4ExpShowAscensionExp
                    | FieldRef::MiniVitalsNumbersOnly
                    | FieldRef::MiniVitalsCurrentOnly
                    | FieldRef::BetrayerShowItems
                    | FieldRef::TextCompact
                    | FieldRef::TargetsShowAppendages
                    | FieldRef::TargetsStatusPosition
            )
        )
    }

    /// Check if the currently focused field is the border style dropdown
    pub fn is_on_border_style(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::BorderStyle))
    }

    /// Check if the currently focused field is the content alignment dropdown
    pub fn is_on_content_align(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::ContentAlign))
    }

    /// Check if the currently focused field is the title alignment dropdown
    pub fn is_on_title_position(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::TitlePosition))
    }

    /// Check if focused on tab bar position dropdown (TabbedText)
    pub fn is_on_tab_bar_position(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::TabBarPosition))
    }

    /// Check if the current field is the Edit Tabs button
    pub fn is_on_edit_tabs(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::EditTabs))
    }

    /// Whether the Streams text field is currently focused (used to gate the
    /// seen-streams picker hotkey).
    pub fn is_on_streams(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::Streams))
    }

    /// Check if the current field is the Edit Indicators button
    pub fn is_on_edit_indicators(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::EditIndicators))
    }

    /// Check if the current field is the Edit Metrics button
    pub fn is_on_edit_metrics(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::EditMetrics))
    }

    /// Check if the current field is the Edit Bar Order button
    pub fn is_on_edit_bar_order(&self) -> bool {
        matches!(
            self.current_field_ref(),
            Some(FieldRef::MiniVitalsEditBarOrder)
        )
    }

    /// Check if the current field is the Perception Sort Direction dropdown
    pub fn is_on_perception_sort_direction(&self) -> bool {
        matches!(
            self.current_field_ref(),
            Some(FieldRef::PerceptionSortDirection)
        )
    }

    /// Check if the current field is the Perception Text Replacements button
    pub fn is_on_perception_replacements(&self) -> bool {
        matches!(
            self.current_field_ref(),
            Some(FieldRef::PerceptionTextReplacements)
        )
    }

    /// Check if the current field is the Perception Short Spell Names checkbox
    pub fn is_on_perception_short_spell_names(&self) -> bool {
        matches!(
            self.current_field_ref(),
            Some(FieldRef::PerceptionUseShortSpellNames)
        )
    }

    /// Toggle the perception short spell names setting
    pub fn toggle_perception_short_spell_names(&mut self) {
        self.perception_use_short_spell_names = !self.perception_use_short_spell_names;
    }

    /// Check if the current field is the Dashboard Layout dropdown
    pub fn is_on_dashboard_layout(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::DashboardLayout))
    }

    /// Cycle through dashboard layout options
    pub fn cycle_dashboard_layout(&mut self) {
        let current = self
            .dashboard_layout_input
            .lines()
            .get(0)
            .map(|s| s.as_str())
            .unwrap_or("horizontal")
            .to_lowercase();
        let options = [
            "horizontal",
            "vertical",
            "flow",
            "grid:2x2",
            "grid:2x3",
            "grid:3x3",
        ];
        let idx = options
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(0);
        let next = options[(idx + 1) % options.len()];
        let mut ta = Self::create_textarea();
        ta.insert_str(next);
        self.dashboard_layout_input = ta;
    }

    /// Cycle to the next/previous border style
    pub fn cycle_border_style(&mut self, reverse: bool) {
        let options = ["single", "double", "rounded", "thick"];
        let current = &self.window_def.base().border_style;
        let len = options.len();
        let current_idx = options
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(current))
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                len - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % len
        };
        self.window_def.base_mut().border_style = options[next_idx].to_string();
    }

    /// Cycle content alignment through the presets
    pub fn cycle_content_align(&mut self, reverse: bool) {
        let current = self.current_content_align_value().to_string();
        let len = CONTENT_ALIGN_OPTIONS.len();
        let current_idx = CONTENT_ALIGN_OPTIONS
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                len - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % len
        };
        let new_value = CONTENT_ALIGN_OPTIONS[next_idx];

        let mut new_input = Self::create_textarea();
        new_input.insert_str(new_value);
        self.content_align_input = new_input;
        self.window_def.base_mut().content_align = Some(new_value.to_string());
    }

    /// Cycle title alignment through the supported positions
    pub fn cycle_title_position(&mut self, reverse: bool) {
        let current = self
            .title_position_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.window_def.base().title_position.clone());

        let len = TITLE_POSITION_OPTIONS.len();
        let current_idx = TITLE_POSITION_OPTIONS
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                len - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % len
        };
        let new_value = TITLE_POSITION_OPTIONS[next_idx];

        let mut ta = Self::create_textarea();
        ta.insert_str(new_value);
        self.title_position_input = ta;
        self.window_def.base_mut().title_position = new_value.to_string();
    }

    /// Cycle tab bar position for tabbed text windows
    pub fn cycle_tab_bar_position(&mut self) {
        let next = match self
            .tab_bar_position_input
            .lines()
            .get(0)
            .map(|s| s.as_str())
            .unwrap_or("top")
        {
            "top" => "bottom",
            _ => "top",
        };
        let mut ta = Self::create_textarea();
        ta.insert_str(next);
        self.tab_bar_position_input = ta;
    }

    /// Cycle perception sort direction
    pub fn cycle_perception_sort_direction(&mut self) {
        let current = self
            .perception_sort_direction_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_else(|| "descending".to_string());

        let len = SORT_DIRECTION_OPTIONS.len();
        let current_idx = SORT_DIRECTION_OPTIONS
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(1); // Default to "descending" if not found
        let next_idx = (current_idx + 1) % len;
        let new_value = SORT_DIRECTION_OPTIONS[next_idx];

        let mut ta = Self::create_textarea();
        ta.insert_str(new_value);
        self.perception_sort_direction_input = ta;
    }
}
