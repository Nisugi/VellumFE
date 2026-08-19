//! Rendering the editor popup: the field grid and every sub-editor
//! overlay, plus the compact field-widget painters.

use super::*;

impl WindowEditor {
    pub fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &EditorTheme) {
        // Center the popup on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(self.popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(self.popup_height)) / 2;
        }

        // Constrain position to screen bounds
        self.popup_x = self
            .popup_x
            .min(area.width.saturating_sub(self.popup_width));
        self.popup_y = self
            .popup_y
            .min(area.height.saturating_sub(self.popup_height));

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: self.popup_width,
            height: self.popup_height,
        };

        Clear.render(popup_area, buf);

        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    let cell = &mut buf[(x, y)];
                    cell.set_char(' ').set_bg(Color::Black);
                }
            }
        }

        let title = if self.is_new {
            " Add Window "
        } else {
            " Edit Window "
        };

        let block = Block::default().borders(Borders::ALL).title(title).style(
            Style::default()
                .bg(Color::Black)
                .fg(crossterm_bridge::to_ratatui_color(theme.border_color)),
        );
        block.render(popup_area, buf);

        // Draw combined bottom border with footer hints
        let inner_width = popup_area.width.saturating_sub(2);
        let help = self.footer_help_text();
        // Use chars().count() not len() - help contains multi-byte Unicode chars like "─"
        let pad_len = inner_width.saturating_sub(1 + help.chars().count() as u16) as usize;
        let pad = "─".repeat(pad_len);
        let mut interior = String::from("─");
        interior.push_str(help);
        interior.push_str(&pad);
        let mut footer_line = String::new();
        footer_line.push('└');
        footer_line.push_str(
            &interior
                .chars()
                .take(inner_width as usize)
                .collect::<String>(),
        );
        footer_line.push('┘');
        buf.set_string(
            popup_area.x,
            popup_area.y + popup_area.height.saturating_sub(1),
            footer_line,
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.border_color)),
        );

        let content = Rect {
            x: popup_area.x + 1,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(2),
            height: popup_area.height.saturating_sub(2),
        };

        if self.is_sub_editor_active() {
            self.render_sub_editor(content, buf, theme);
        } else {
            self.render_fields(content, buf, theme);
        }
    }

    pub(super) fn render_sub_editor(&mut self, area: Rect, buf: &mut Buffer, theme: &EditorTheme) {
        if let Some(mut editor) = self.tab_editor.take() {
            self.render_tab_editor(area, buf, theme, &mut editor);
            self.tab_editor = Some(editor);
            return;
        }

        if let Some(mut editor) = self.indicator_editor.take() {
            self.render_indicator_editor(area, buf, theme, &mut editor);
            self.indicator_editor = Some(editor);
            return;
        }

        if let Some(mut picker) = self.stream_picker.take() {
            self.render_stream_picker(area, buf, theme, &mut picker);
            self.stream_picker = Some(picker);
            return;
        }

        if let Some(mut editor) = self.performance_metrics_editor.take() {
            self.render_performance_metrics_editor(area, buf, theme, &mut editor);
            self.performance_metrics_editor = Some(editor);
            return;
        }

        if let Some(mut editor) = self.text_replacements_editor.take() {
            self.render_text_replacements_editor(area, buf, theme, &mut editor);
            self.text_replacements_editor = Some(editor);
            return;
        }

        if let Some(mut editor) = self.bar_order_editor.take() {
            self.render_bar_order_editor(area, buf, theme, &mut editor);
            self.bar_order_editor = Some(editor);
        }
    }

    pub(super) fn render_bar_order_editor(
        &self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut BarOrderEditor,
    ) {
        let header_style = Style::default().fg(crossterm_bridge::to_ratatui_color(
            theme.section_header_color,
        ));
        let label_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color));
        let focused_style = Style::default().fg(crossterm_bridge::to_ratatui_color(
            theme.focused_label_color,
        ));
        let text_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_color));
        let cursor_style = Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(crossterm_bridge::to_ratatui_color(theme.cursor_color));

        // Clear click areas for fresh population
        editor.click_areas.clear();

        // Title with enabled count indicator
        let title = format!(
            "Bar Order Editor ({}/{} enabled)",
            editor.enabled_count(),
            BarOrderEditor::MAX_ENABLED
        );
        buf.set_string(area.x + 1, area.y, &title, header_style);

        // Two-column layout:
        // Left column: "  [x] Health" (toggles)
        // Right column: "██ red_____" (color preview + input)
        let toggle_col_x = area.x + 1;
        let color_col_x = area.x + 22; // Start color column after toggle column
        let color_input_width = 12usize; // Width for color input

        let max_rows = area.height.saturating_sub(2);
        for (idx, bar) in editor.bars.iter().enumerate() {
            if idx as u16 >= max_rows {
                break;
            }
            let y = area.y + 1 + idx as u16;
            let is_sel = idx == editor.selected;
            let is_toggle_focus = is_sel && editor.focus == BarOrderEditorFocus::Toggle;
            let is_color_focus = is_sel && editor.focus == BarOrderEditorFocus::Color;

            // Left column: Toggle checkbox
            let prefix = if is_toggle_focus { "→ " } else { "  " };
            let checkbox = if bar.enabled { "[x]" } else { "[ ]" };
            let toggle_style = if is_toggle_focus {
                focused_style
            } else {
                label_style
            };
            let toggle_text = format!("{}{} {}", prefix, checkbox, bar.label);
            buf.set_string(toggle_col_x, y, &toggle_text, toggle_style);

            // Store toggle click area
            let toggle_rect = (toggle_col_x, y, 20, 1);

            // Right column: Color preview swatch + color input
            let color_str = if is_color_focus {
                // Show current input for selected bar
                editor.color_input.lines().join("")
            } else {
                bar.color
                    .clone()
                    .unwrap_or_else(|| BarOrderEditor::default_color_for_id(&bar.id).to_string())
            };

            // Draw color preview swatch (2 chars)
            let preview_color_str = if color_str.trim().is_empty() {
                BarOrderEditor::default_color_for_id(&bar.id)
            } else {
                color_str.trim()
            };
            if let Some(ratatui_color) =
                crate::frontend::tui::colors::parse_color_to_ratatui(preview_color_str)
            {
                let swatch_style = Style::default().bg(ratatui_color);
                buf.set_string(color_col_x, y, "  ", swatch_style);
            } else {
                buf.set_string(color_col_x, y, "??", label_style);
            }

            // Draw color input field
            let input_x = color_col_x + 3;
            if is_color_focus {
                // Render with cursor for active editing
                let cursor_pos = editor.color_input.cursor().1;
                let chars: Vec<char> = color_str.chars().collect();

                for (i, ch) in chars.iter().enumerate().take(color_input_width) {
                    let x = input_x + i as u16;
                    if x < area.x + area.width {
                        if i == cursor_pos {
                            buf.set_string(x, y, &ch.to_string(), cursor_style);
                        } else {
                            buf.set_string(x, y, &ch.to_string(), focused_style);
                        }
                    }
                }
                // Cursor at end
                if cursor_pos >= chars.len() {
                    let x = input_x + chars.len() as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, " ", cursor_style);
                    }
                }
                // Fill remaining with underscores
                let filled = chars.len().min(color_input_width);
                let start = if cursor_pos >= chars.len() {
                    filled + 1
                } else {
                    filled
                };
                for i in start..color_input_width {
                    let x = input_x + i as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, "_", text_style);
                    }
                }
            } else {
                // Just show the color value
                let display: String = color_str.chars().take(color_input_width).collect();
                let padded = format!("{:<width$}", display, width = color_input_width);
                let style = if is_sel { focused_style } else { text_style };
                buf.set_string(input_x, y, &padded, style);
            }

            // Store color click area
            let color_rect = (color_col_x, y, (3 + color_input_width) as u16, 1);

            // Add click areas for this bar
            editor.click_areas.push((idx, toggle_rect, color_rect));
        }
    }

    pub(super) fn render_tab_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut TabEditor,
    ) {
        let header_style = Style::default().fg(crossterm_bridge::to_ratatui_color(
            theme.section_header_color,
        ));
        buf.set_string(area.x + 1, area.y, "Tab Editor", header_style);

        match editor.mode {
            TabEditorMode::List => {
                // Clear click areas for fresh population
                editor.click_areas.clear();

                let max_rows = area.height.saturating_sub(2);
                let available_width = area.width.saturating_sub(2) as usize;
                let name_col_width = available_width
                    .saturating_sub(6)
                    .min(24)
                    .max(available_width.min(8));
                let stream_col_width = available_width.saturating_sub(name_col_width + 4);
                for (idx, tab) in editor.tabs.iter().enumerate() {
                    if idx as u16 >= max_rows {
                        break;
                    }
                    let y = area.y + 1 + idx as u16;
                    let is_sel = idx == editor.selected;
                    let prefix = if is_sel { "> " } else { "  " };
                    let color = if is_sel {
                        crossterm_bridge::to_ratatui_color(theme.focused_label_color)
                    } else {
                        crossterm_bridge::to_ratatui_color(theme.label_color)
                    };
                    let stream_display = if tab.streams.is_empty() {
                        "-".to_string()
                    } else {
                        tab.streams.join(", ")
                    };
                    let name_text: String = tab.name.chars().take(name_col_width).collect();
                    let stream_text: String =
                        stream_display.chars().take(stream_col_width).collect();
                    let line = format!(
                        "{}{:name_width$} ->  {}",
                        prefix,
                        name_text,
                        stream_text,
                        name_width = name_col_width
                    );
                    buf.set_string(
                        area.x + 1,
                        y,
                        self.truncate_to_width(&line, available_width as u16),
                        Style::default().fg(color),
                    );
                    // Add click area for this tab row
                    editor
                        .click_areas
                        .push((idx, y, area.x + 1, available_width as u16));
                }
            }
            TabEditorMode::Form => {
                let y = area.y + 2;
                self.render_tab_editor_input(
                    "Tab Name",
                    &editor.name_input,
                    area.x + 1,
                    y,
                    area.width.saturating_sub(2),
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::Name),
                );
                self.render_tab_editor_input(
                    "Stream",
                    &editor.streams_input,
                    area.x + 1,
                    y + 1,
                    area.width.saturating_sub(2),
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::Streams),
                );

                let ts_label = "Timestamps";
                self.render_tab_editor_checkbox(
                    ts_label,
                    editor.show_timestamps,
                    area.x + 1,
                    y + 2,
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::Timestamps),
                );

                let ignore_label = "Ignore Activity";
                self.render_tab_editor_checkbox(
                    ignore_label,
                    editor.ignore_activity,
                    area.x + 1,
                    y + 3,
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::IgnoreActivity),
                );
            }
        }
    }

    pub(super) fn render_indicator_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut IndicatorEditor,
    ) {
        let header_style = Style::default().fg(crossterm_bridge::to_ratatui_color(
            theme.section_header_color,
        ));
        buf.set_string(area.x + 1, area.y, "Indicator Selector", header_style);

        match editor.mode {
            IndicatorEditorMode::List => {
                // Clear click areas for fresh population
                editor.click_areas.clear();

                let max_rows = area.height.saturating_sub(2);
                for (idx, ind) in editor.indicators.iter().enumerate() {
                    if idx as u16 >= max_rows {
                        break;
                    }
                    let y = area.y + 1 + idx as u16;
                    let is_sel = idx == editor.selected;
                    let prefix = if is_sel { "> " } else { "  " };
                    let color = if is_sel {
                        crossterm_bridge::to_ratatui_color(theme.focused_label_color)
                    } else {
                        crossterm_bridge::to_ratatui_color(theme.label_color)
                    };
                    let icon = if let Some(ch) = Self::parse_icon_char(&ind.icon) {
                        ch.to_string()
                    } else if ind.icon.is_empty() {
                        "?".to_string()
                    } else {
                        ind.icon.clone()
                    };
                    let enabled_marker = if ind.enabled { "[x]" } else { "[ ]" };
                    let mut line = format!("{}{} {} {}", prefix, enabled_marker, icon, ind.id);
                    let max_width = area.width.saturating_sub(2) as usize;
                    if line.chars().count() > max_width {
                        line = line.chars().take(max_width).collect();
                    }
                    let mut style = Style::default().fg(color);
                    if !ind.enabled {
                        style = style.add_modifier(Modifier::DIM);
                    }
                    buf.set_string(area.x + 1, y, line, style);
                    // Add click area for this indicator row
                    editor
                        .click_areas
                        .push((idx, y, area.x + 1, max_width as u16));
                }
            }
            IndicatorEditorMode::Form => {
                let y = area.y + 1;
                self.render_textarea_compact(
                    0,
                    "Id:",
                    &editor.id_input,
                    area.x + 1,
                    y,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, IndicatorFormField::Id),
                );
                self.render_textarea_compact(
                    0,
                    "Icon:",
                    &editor.icon_input,
                    area.x + 1,
                    y + 2,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, IndicatorFormField::Icon),
                );
                self.render_textarea_compact(
                    0,
                    "Colors:",
                    &editor.colors_input,
                    area.x + 1,
                    y + 4,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, IndicatorFormField::Colors),
                );
                let value = editor
                    .colors_input
                    .lines()
                    .get(0)
                    .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
                    .unwrap_or_default();
                let preview_x = area.x + 1 + 2 + "Colors:".len() as u16 + 1 + 10;
                self.render_color_preview(&value, preview_x, y + 4, buf, theme);

                let footer = "Enter: Save | Esc: Cancel | Tab/Shift+Tab: Next/Prev";
                let footer_style =
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color));
                buf.set_string(
                    area.x + 1,
                    area.y + area.height.saturating_sub(1),
                    self.truncate_to_width(footer, area.width.saturating_sub(2)),
                    footer_style,
                );
            }
        }
    }

    pub(super) fn render_stream_picker(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        picker: &mut StreamPicker,
    ) {
        let header_style = Style::default().fg(crossterm_bridge::to_ratatui_color(
            theme.section_header_color,
        ));
        buf.set_string(
            area.x + 1,
            area.y,
            "Streams seen this session",
            header_style,
        );

        picker.click_areas.clear();

        if picker.streams.is_empty() {
            let msg_style = Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.label_color))
                .add_modifier(Modifier::DIM);
            buf.set_string(
                area.x + 1,
                area.y + 2,
                "(No custom streams seen yet)",
                msg_style,
            );
            return;
        }

        let max_rows = area.height.saturating_sub(2);
        for (idx, (id, label)) in picker.streams.iter().enumerate() {
            if idx as u16 >= max_rows {
                break;
            }
            let y = area.y + 1 + idx as u16;
            let is_sel = idx == picker.selected;
            let prefix = if is_sel { "> " } else { "  " };
            let color = if is_sel {
                crossterm_bridge::to_ratatui_color(theme.focused_label_color)
            } else {
                crossterm_bridge::to_ratatui_color(theme.label_color)
            };
            let line = match label {
                Some(label) => format!("{}{} ({})", prefix, id, label),
                None => format!("{}{}", prefix, id),
            };
            let text = self.truncate_to_width(&line, area.width.saturating_sub(2));
            let width = text.chars().count() as u16;
            buf.set_string(area.x + 1, y, text, Style::default().fg(color));
            picker.click_areas.push((idx, y, area.x + 1, width));
        }
    }

    pub(super) fn render_performance_metrics_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut PerformanceMetricsEditor,
    ) {
        let header_style = Style::default().fg(crossterm_bridge::to_ratatui_color(
            theme.section_header_color,
        ));
        buf.set_string(area.x + 1, area.y, "Performance Metrics", header_style);

        let max_rows = area.height.saturating_sub(2);
        for (idx, item) in editor.items.iter().enumerate() {
            if idx as u16 >= max_rows {
                break;
            }
            let y = area.y + 1 + idx as u16;
            let is_sel = idx == editor.selected;
            let prefix = if is_sel { "> " } else { "  " };
            let marker = if item.enabled { "[x]" } else { "[ ]" };
            let color = if is_sel {
                crossterm_bridge::to_ratatui_color(theme.focused_label_color)
            } else {
                crossterm_bridge::to_ratatui_color(theme.label_color)
            };
            let line = format!("{}{} {}", prefix, marker, item.group.label());
            buf.set_string(
                area.x + 1,
                y,
                self.truncate_to_width(&line, area.width.saturating_sub(2)),
                Style::default().fg(color),
            );
        }
    }

    pub(super) fn render_text_replacements_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut TextReplacementsEditor,
    ) {
        let header_style = Style::default().fg(crossterm_bridge::to_ratatui_color(
            theme.section_header_color,
        ));
        buf.set_string(area.x + 1, area.y, "Text Replacements Editor", header_style);

        match editor.mode {
            TextReplacementsEditorMode::List => {
                // Footer help is shown in the border, no need for separate footer here

                if editor.replacements.is_empty() {
                    let empty_msg = "(No replacements defined - press 'a' to add)";
                    let msg_style = Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.label_color))
                        .add_modifier(Modifier::DIM);
                    buf.set_string(area.x + 1, area.y + 2, empty_msg, msg_style);
                    return;
                }

                let max_rows = area.height.saturating_sub(3) as usize;
                let available_width = area.width.saturating_sub(4) as usize;
                let pattern_width = (available_width / 2).min(30);
                let replace_width = available_width.saturating_sub(pattern_width + 4);

                for (idx, item) in editor.replacements.iter().enumerate() {
                    if idx >= max_rows {
                        break;
                    }
                    let y = area.y + 1 + idx as u16;
                    let is_sel = idx == editor.selected;
                    let prefix = if is_sel { "> " } else { "  " };
                    let color = if is_sel {
                        crossterm_bridge::to_ratatui_color(theme.focused_label_color)
                    } else {
                        crossterm_bridge::to_ratatui_color(theme.label_color)
                    };

                    // Format: pattern → replace (or pattern → (remove) if replace is empty)
                    let pattern_display: String =
                        item.pattern.chars().take(pattern_width).collect();
                    let replace_display = if item.replace.is_empty() {
                        "(remove)".to_string()
                    } else {
                        item.replace.chars().take(replace_width).collect()
                    };
                    let line = format!("{}{} → {}", prefix, pattern_display, replace_display);
                    buf.set_string(area.x + 1, y, &line, Style::default().fg(color));
                }
            }
            TextReplacementsEditorMode::Form => {
                let y = area.y + 1;
                self.render_textarea_compact(
                    0,
                    "Pattern:",
                    &editor.pattern_input,
                    area.x + 1,
                    y,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, TextReplacementsFormField::Pattern),
                );
                self.render_textarea_compact(
                    0,
                    "Replace:",
                    &editor.replace_input,
                    area.x + 1,
                    y + 2,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, TextReplacementsFormField::Replace),
                );

                let hint = "(leave Replace empty to remove matched text)";
                let hint_style = Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.label_color))
                    .add_modifier(Modifier::DIM);
                buf.set_string(area.x + 1, y + 4, hint, hint_style);

                let footer = "Enter: Save | Esc: Cancel | Tab: Next Field";
                let footer_style =
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color));
                buf.set_string(
                    area.x + 1,
                    area.y + area.height.saturating_sub(1),
                    self.truncate_to_width(footer, area.width.saturating_sub(2)),
                    footer_style,
                );
            }
        }
    }

    pub(super) fn truncate_to_width(&self, text: &str, width: u16) -> String {
        if width == 0 {
            return String::new();
        }
        let width_usize = width as usize;
        if text.chars().count() <= width_usize {
            text.to_string()
        } else {
            text.chars().take(width_usize).collect()
        }
    }

    pub(super) fn render_fields(&mut self, area: Rect, buf: &mut Buffer, theme: &EditorTheme) {
        // Clear click areas for fresh population
        self.field_click_areas.clear();

        let left_x = area.x + 1;
        let right_x = area.x + 38;
        let geom_x2 = left_x + 16;
        let column_width = 30;

        let mut left_y = area.y + 1;
        let mut right_y = area.y + 1;

        let is_focus = |f: FieldRef, focused: usize| focused == f.legacy_field_id();

        // Left column: Identity + geometry
        self.render_textarea_compact(
            FieldRef::Name.legacy_field_id(),
            " Name:",
            &self.name_input,
            left_x,
            left_y,
            24,
            buf,
            theme,
            is_focus(FieldRef::Name, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::Name));
        left_y += 1;

        self.render_textarea_compact(
            FieldRef::Title.legacy_field_id(),
            "Title:",
            &self.title_input,
            left_x,
            left_y,
            24,
            buf,
            theme,
            is_focus(FieldRef::Title, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::Title));
        left_y += 1;

        // Title align
        self.render_dropdown_compact(
            FieldRef::TitlePosition.legacy_field_id(),
            "Title Align:",
            self.title_position_input
                .lines()
                .get(0)
                .map(|s| s.as_str())
                .unwrap_or("top-left"),
            left_x,
            left_y,
            14,
            buf,
            theme,
            is_focus(FieldRef::TitlePosition, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::TitlePosition));
        left_y += 1;

        // Content align
        self.render_dropdown_compact(
            FieldRef::ContentAlign.legacy_field_id(),
            "Content Align:",
            self.current_content_align_value(),
            left_x,
            left_y,
            14,
            buf,
            theme,
            is_focus(FieldRef::ContentAlign, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::ContentAlign));
        left_y += 1;

        // Border style
        self.render_dropdown_compact(
            FieldRef::BorderStyle.legacy_field_id(),
            " Border Style:",
            &self.window_def.base().border_style,
            left_x,
            left_y,
            10,
            buf,
            theme,
            is_focus(FieldRef::BorderStyle, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::BorderStyle));
        left_y += 2;

        // Row / Col
        self.render_textarea_compact(
            FieldRef::Row.legacy_field_id(),
            "  Row:",
            &self.row_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Row, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::Row));
        self.render_textarea_compact(
            FieldRef::Col.legacy_field_id(),
            "  Col:",
            &self.col_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Col, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, geom_x2, FieldRef::Col));
        left_y += 1;

        // Rows / Cols
        self.render_textarea_compact(
            FieldRef::Rows.legacy_field_id(),
            " Rows:",
            &self.rows_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Rows, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::Rows));
        self.render_textarea_compact(
            FieldRef::Cols.legacy_field_id(),
            " Cols:",
            &self.cols_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Cols, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, geom_x2, FieldRef::Cols));
        left_y += 1;

        // Min/Max constraints
        self.render_textarea_compact(
            FieldRef::MinRows.legacy_field_id(),
            "  Min:",
            &self.min_rows_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MinRows, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::MinRows));
        self.render_textarea_compact(
            FieldRef::MinCols.legacy_field_id(),
            "  Min:",
            &self.min_cols_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MinCols, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, geom_x2, FieldRef::MinCols));
        left_y += 1;

        self.render_textarea_compact(
            FieldRef::MaxRows.legacy_field_id(),
            "  Max:",
            &self.max_rows_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MaxRows, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, left_x, FieldRef::MaxRows));
        self.render_textarea_compact(
            FieldRef::MaxCols.legacy_field_id(),
            "  Max:",
            &self.max_cols_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MaxCols, self.focused_field),
        );
        self.field_click_areas
            .push((left_y, geom_x2, FieldRef::MaxCols));
        left_y += 2;

        // Right column: appearance
        self.render_checkbox_compact(
            FieldRef::Locked.legacy_field_id(),
            "Lock Window",
            self.window_def.base().locked,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::Locked, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::Locked));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::ShowTitle.legacy_field_id(),
            "Show Title",
            self.window_def.base().show_title,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::ShowTitle, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::ShowTitle));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::TransparentBg.legacy_field_id(),
            "Transparent BG",
            self.window_def.base().transparent_background,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::TransparentBg, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::TransparentBg));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::ShowBorder.legacy_field_id(),
            "Show Border",
            self.window_def.base().show_border,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::ShowBorder, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::ShowBorder));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderTop.legacy_field_id(),
            "Top Border",
            self.window_def.base().border_sides.top,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderTop, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::BorderTop));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderBottom.legacy_field_id(),
            "Bottom Border",
            self.window_def.base().border_sides.bottom,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderBottom, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::BorderBottom));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderLeft.legacy_field_id(),
            "Left Border",
            self.window_def.base().border_sides.left,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderLeft, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::BorderLeft));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderRight.legacy_field_id(),
            "Right Border",
            self.window_def.base().border_sides.right,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderRight, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::BorderRight));
        right_y += 1;

        self.render_color_field(
            FieldRef::BgColor.legacy_field_id(),
            "BG Color",
            &self.bg_color_input,
            right_x,
            right_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::BgColor, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::BgColor));
        right_y += 1;

        self.render_color_field(
            FieldRef::BorderColor.legacy_field_id(),
            "Border",
            &self.border_color_input,
            right_x,
            right_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::BorderColor, self.focused_field),
        );
        self.field_click_areas
            .push((right_y, right_x, FieldRef::BorderColor));

        // Special section
        let special_y = left_y.max(right_y) + 1;
        let mut special_row = special_y;
        match &self.window_def {
            WindowDef::CommandInput { .. } => {
                // Text color on first row, right column
                self.render_color_field(
                    FieldRef::TextColor.legacy_field_id(),
                    "Text",
                    &self.text_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TextColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::TextColor));
                special_row += 1;

                // Icon text + cursor foreground
                self.render_textarea_compact(
                    FieldRef::PromptIcon.legacy_field_id(),
                    "Icon:",
                    &self.prompt_icon_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::PromptIcon, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::PromptIcon));
                self.render_color_field(
                    FieldRef::CursorColor.legacy_field_id(),
                    "Cursor FG",
                    &self.cursor_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CursorColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::CursorColor));
                special_row += 1;

                // Icon color + cursor background
                self.render_color_field(
                    FieldRef::PromptIconColor.legacy_field_id(),
                    "Icon",
                    &self.prompt_icon_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::PromptIconColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::PromptIconColor));
                self.render_color_field(
                    FieldRef::CursorBg.legacy_field_id(),
                    "Cursor BG",
                    &self.cursor_bg_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CursorBg, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::CursorBg));
                special_row += 1;

                // History-suggestion ghost color (falls back to the theme's
                // text_secondary when blank).
                self.render_color_field(
                    FieldRef::CompletionColor.legacy_field_id(),
                    "Suggest",
                    &self.completion_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CompletionColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::CompletionColor));
            }
            WindowDef::Text { .. } => {
                // Bounty window is special: hide Streams and BufferSize
                let is_bounty = self.window_def.base().name.eq_ignore_ascii_case("bounty");

                if is_bounty {
                    // Bounty layout: Wordwrap (left), Timestamps (right) on row 1
                    // TextCompact (left) on row 2
                    self.render_checkbox_compact(
                        FieldRef::Wordwrap.legacy_field_id(),
                        "Wordwrap",
                        self.text_wordwrap,
                        left_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Wordwrap, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, left_x, FieldRef::Wordwrap));
                    self.render_checkbox_compact(
                        FieldRef::Timestamps.legacy_field_id(),
                        "Timestamps",
                        self.text_show_timestamps,
                        right_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Timestamps, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, right_x, FieldRef::Timestamps));
                    special_row += 1;
                    self.render_checkbox_compact(
                        FieldRef::TextCompact.legacy_field_id(),
                        "Compact",
                        self.text_compact,
                        left_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::TextCompact, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, left_x, FieldRef::TextCompact));
                } else {
                    // Standard text window layout
                    self.render_textarea_compact(
                        FieldRef::Streams.legacy_field_id(),
                        "Streams:",
                        &self.streams_input,
                        left_x,
                        special_row,
                        column_width as usize,
                        buf,
                        theme,
                        is_focus(FieldRef::Streams, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, left_x, FieldRef::Streams));
                    self.render_checkbox_compact(
                        FieldRef::Wordwrap.legacy_field_id(),
                        "Wordwrap",
                        self.text_wordwrap,
                        right_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Wordwrap, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, right_x, FieldRef::Wordwrap));
                    special_row += 1;
                    self.render_textarea_compact(
                        FieldRef::BufferSize.legacy_field_id(),
                        "Buffer Size:",
                        &self.buffer_size_input,
                        left_x,
                        special_row,
                        8,
                        buf,
                        theme,
                        is_focus(FieldRef::BufferSize, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, left_x, FieldRef::BufferSize));
                    self.render_checkbox_compact(
                        FieldRef::Timestamps.legacy_field_id(),
                        "Timestamps",
                        self.text_show_timestamps,
                        right_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Timestamps, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, right_x, FieldRef::Timestamps));
                    special_row += 1;
                    // Compact mode checkbox
                    self.render_checkbox_compact(
                        FieldRef::TextCompact.legacy_field_id(),
                        "Compact",
                        self.text_compact,
                        left_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::TextCompact, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, left_x, FieldRef::TextCompact));
                    // Speak new lines (TTS opt-in), right column of the same row.
                    self.render_checkbox_compact(
                        FieldRef::TtsSpeak.legacy_field_id(),
                        "Speak (TTS)",
                        self.window_def.base().tts_speak,
                        right_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::TtsSpeak, self.focused_field),
                    );
                    self.field_click_areas
                        .push((special_row, right_x, FieldRef::TtsSpeak));
                }
            }
            WindowDef::Inventory { .. } | WindowDef::Reserve { .. } => {
                self.render_textarea_compact(
                    FieldRef::Streams.legacy_field_id(),
                    "Streams:",
                    &self.streams_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::Streams, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::Streams));
                self.render_checkbox_compact(
                    FieldRef::Wordwrap.legacy_field_id(),
                    "Wordwrap",
                    self.text_wordwrap,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::Wordwrap, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::Wordwrap));
                special_row += 1;
                self.render_textarea_compact(
                    FieldRef::BufferSize.legacy_field_id(),
                    "Buffer Size:",
                    &self.buffer_size_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::BufferSize, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::BufferSize));
                self.render_checkbox_compact(
                    FieldRef::Timestamps.legacy_field_id(),
                    "Timestamps",
                    self.text_show_timestamps,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::Timestamps, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::Timestamps));
            }
            WindowDef::Targets { .. } => {
                self.render_textarea_compact(
                    FieldRef::EntityId.legacy_field_id(),
                    "Entity ID:",
                    &self.entity_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::EntityId, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::EntityId));
                self.render_checkbox_compact(
                    FieldRef::TargetsShowAppendages.legacy_field_id(),
                    "Show Appendages",
                    self.targets_show_arms_count,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::TargetsShowAppendages, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row,
                    right_x,
                    FieldRef::TargetsShowAppendages,
                ));

                // Second row: Status Position dropdown
                let special_row_2 = special_row + 1;
                // Convert internal value to display text
                let status_display = if self.targets_status_position == "start" {
                    "Left"
                } else {
                    "Right"
                };
                self.render_dropdown_compact(
                    FieldRef::TargetsStatusPosition.legacy_field_id(),
                    "Status Pos:",
                    status_display,
                    left_x,
                    special_row_2,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::TargetsStatusPosition, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row_2,
                    left_x,
                    FieldRef::TargetsStatusPosition,
                ));
            }
            WindowDef::Players { .. } => {
                self.render_textarea_compact(
                    FieldRef::EntityId.legacy_field_id(),
                    "Entity ID:",
                    &self.entity_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::EntityId, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::EntityId));
            }
            WindowDef::TabbedText { .. } => {
                let special_left_x = left_x + 2;
                self.render_dropdown_compact(
                    FieldRef::TabBarPosition.legacy_field_id(),
                    "Tab Bar Pos:",
                    self.tab_bar_position_input
                        .lines()
                        .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or("top"),
                    special_left_x,
                    special_row,
                    10,
                    buf,
                    theme,
                    is_focus(FieldRef::TabBarPosition, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row,
                    special_left_x,
                    FieldRef::TabBarPosition,
                ));
                self.render_color_field(
                    FieldRef::TabActiveColor.legacy_field_id(),
                    "Active",
                    &self.tab_active_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabActiveColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::TabActiveColor));
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::TabSeparator.legacy_field_id(),
                    "Tab Separator",
                    self.tab_separator,
                    special_left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::TabSeparator, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, special_left_x, FieldRef::TabSeparator));
                self.render_color_field(
                    FieldRef::TabInactiveColor.legacy_field_id(),
                    "Inactive",
                    &self.tab_inactive_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabInactiveColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::TabInactiveColor));
                special_row += 1;
                self.render_textarea_compact(
                    FieldRef::TabUnreadPrefix.legacy_field_id(),
                    "New Msg Icon:",
                    &self.tab_unread_prefix_input,
                    special_left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabUnreadPrefix, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row,
                    special_left_x,
                    FieldRef::TabUnreadPrefix,
                ));
                self.render_color_field(
                    FieldRef::TabUnreadColor.legacy_field_id(),
                    "Unread",
                    &self.tab_unread_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabUnreadColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::TabUnreadColor));
                special_row += 1;
                self.render_button(
                    FieldRef::EditTabs.legacy_field_id(),
                    "[ Edit Tabs ]",
                    special_left_x,
                    special_row,
                    buf,
                    theme,
                    is_focus(FieldRef::EditTabs, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, special_left_x, FieldRef::EditTabs));
            }
            WindowDef::Room { .. } => {
                self.render_checkbox_compact(
                    FieldRef::ShowName.legacy_field_id(),
                    "Show Name",
                    self.show_name,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowName, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::ShowName));
                self.render_checkbox_compact(
                    FieldRef::ShowDesc.legacy_field_id(),
                    "Show Desc",
                    self.show_desc,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowDesc, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::ShowDesc));
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ShowObjs.legacy_field_id(),
                    "Show Objects",
                    self.show_objs,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowObjs, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::ShowObjs));
                self.render_checkbox_compact(
                    FieldRef::ShowPlayers.legacy_field_id(),
                    "Show Players",
                    self.show_players,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowPlayers, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::ShowPlayers));
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ShowExits.legacy_field_id(),
                    "Show Exits",
                    self.show_exits,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowExits, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::ShowExits));
            }
            WindowDef::Progress { .. } => {
                self.render_textarea_compact(
                    FieldRef::ProgressId.legacy_field_id(),
                    "Progress ID:",
                    &self.progress_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressId, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::ProgressId));
                self.render_color_field(
                    FieldRef::TextColor.legacy_field_id(),
                    "Text Color",
                    &self.text_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TextColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::TextColor));
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ProgressNumbersOnly.legacy_field_id(),
                    "Numbers Only",
                    self.progress_numbers_only,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressNumbersOnly, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::ProgressNumbersOnly));
                self.render_color_field(
                    FieldRef::ProgressColor.legacy_field_id(),
                    "Bar Color",
                    &self.progress_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::ProgressColor));
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ProgressCurrentOnly.legacy_field_id(),
                    "Current Only",
                    self.progress_current_only,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressCurrentOnly, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::ProgressCurrentOnly));
            }
            WindowDef::Countdown { .. } => {
                self.render_textarea_compact(
                    FieldRef::CountdownIcon.legacy_field_id(),
                    "Icon:",
                    &self.countdown_icon_input,
                    left_x,
                    special_row,
                    4,
                    buf,
                    theme,
                    is_focus(FieldRef::CountdownIcon, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::CountdownIcon));
                self.render_textarea_compact(
                    FieldRef::CountdownId.legacy_field_id(),
                    "Countdown ID:",
                    &self.countdown_id_input,
                    right_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::CountdownId, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::CountdownId));
                special_row += 1;
                self.render_color_field(
                    FieldRef::CountdownColor.legacy_field_id(),
                    "Icon Color",
                    &self.countdown_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CountdownColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::CountdownColor));
                self.render_color_field(
                    FieldRef::CountdownBgColor.legacy_field_id(),
                    "BG Color",
                    &self.countdown_bg_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CountdownBgColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::CountdownBgColor));
            }
            WindowDef::Compass { .. } => {
                // Clear left column row for a clean right-column layout
                buf.set_string(
                    left_x,
                    special_row,
                    " ".repeat(column_width as usize),
                    Style::default(),
                );
                self.render_color_field(
                    FieldRef::CompassActiveColor.legacy_field_id(),
                    "Active:",
                    &self.compass_active_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CompassActiveColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::CompassActiveColor));
                special_row += 1;
                self.render_color_field(
                    FieldRef::CompassInactiveColor.legacy_field_id(),
                    "Inactive:",
                    &self.compass_inactive_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CompassInactiveColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::CompassInactiveColor));
            }
            WindowDef::InjuryDoll { .. } => {
                self.render_color_field(
                    FieldRef::Injury1Color.legacy_field_id(),
                    "Wound1",
                    &self.injury1_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Injury1Color, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::Injury1Color));
                self.render_color_field(
                    FieldRef::Scar1Color.legacy_field_id(),
                    "Scar1",
                    &self.scar1_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Scar1Color, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::Scar1Color));
                special_row += 1;
                self.render_color_field(
                    FieldRef::Injury2Color.legacy_field_id(),
                    "Wound2",
                    &self.injury2_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Injury2Color, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::Injury2Color));
                self.render_color_field(
                    FieldRef::Scar2Color.legacy_field_id(),
                    "Scar2",
                    &self.scar2_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Scar2Color, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::Scar2Color));
                special_row += 1;
                self.render_color_field(
                    FieldRef::Injury3Color.legacy_field_id(),
                    "Wound3",
                    &self.injury3_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Injury3Color, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::Injury3Color));
                self.render_color_field(
                    FieldRef::Scar3Color.legacy_field_id(),
                    "Scar3",
                    &self.scar3_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Scar3Color, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::Scar3Color));
                special_row += 1;
                self.render_color_field(
                    FieldRef::InjuryDefaultColor.legacy_field_id(),
                    "Uninjured",
                    &self.injury_default_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::InjuryDefaultColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::InjuryDefaultColor));
            }
            WindowDef::Indicator { .. } => {
                self.render_textarea_compact(
                    FieldRef::IndicatorId.legacy_field_id(),
                    "Indicator ID:",
                    &self.indicator_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorId, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::IndicatorId));
                special_row += 1;
                self.render_textarea_compact(
                    FieldRef::IndicatorIcon.legacy_field_id(),
                    "Icon:",
                    &self.indicator_icon_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorIcon, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::IndicatorIcon));
                if let Some(icon_char) = Self::parse_icon_char(
                    self.indicator_icon_input
                        .lines()
                        .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ) {
                    let preview_x = left_x + 2 + "Icon:".len() as u16 + 1 + column_width + 1;
                    if preview_x < buf.area().width && special_row < buf.area().height {
                        buf[(preview_x, special_row)].set_char(icon_char);
                        buf[(preview_x, special_row)]
                            .set_fg(crossterm_bridge::to_ratatui_color(theme.text_color));
                    }
                }
                self.render_color_field(
                    FieldRef::IndicatorActiveColor.legacy_field_id(),
                    "Active:",
                    &self.indicator_active_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorActiveColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::IndicatorActiveColor));
                special_row += 1;
                self.render_color_field(
                    FieldRef::IndicatorInactiveColor.legacy_field_id(),
                    "Inactive:",
                    &self.indicator_inactive_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorInactiveColor, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row,
                    right_x,
                    FieldRef::IndicatorInactiveColor,
                ));
            }
            WindowDef::Hand { .. } => {
                self.render_textarea_compact(
                    FieldRef::HandIcon.legacy_field_id(),
                    "Icon:",
                    &self.hand_icon_input,
                    left_x,
                    special_row,
                    6,
                    buf,
                    theme,
                    is_focus(FieldRef::HandIcon, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::HandIcon));
                self.render_color_field(
                    FieldRef::HandIconColor.legacy_field_id(),
                    "Icon Color",
                    &self.hand_icon_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::HandIconColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::HandIconColor));
                special_row += 1;
                self.render_color_field(
                    FieldRef::HandTextColor.legacy_field_id(),
                    "Text Color",
                    &self.hand_text_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::HandTextColor, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::HandTextColor));
            }
            WindowDef::Dashboard { .. } => {
                self.render_dropdown_compact(
                    FieldRef::DashboardLayout.legacy_field_id(),
                    "Layout:",
                    self.dashboard_layout_input
                        .lines()
                        .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or("horizontal"),
                    left_x,
                    special_row,
                    12,
                    buf,
                    theme,
                    is_focus(FieldRef::DashboardLayout, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::DashboardLayout));
                self.render_textarea_compact(
                    FieldRef::DashboardSpacing.legacy_field_id(),
                    "Spacing:",
                    &self.dashboard_spacing_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::DashboardSpacing, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::DashboardSpacing));
                special_row += 1;
                self.render_button(
                    FieldRef::EditIndicators.legacy_field_id(),
                    "[ Edit Indicators ]",
                    left_x,
                    special_row,
                    buf,
                    theme,
                    is_focus(FieldRef::EditIndicators, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::EditIndicators));
                self.render_checkbox_compact(
                    FieldRef::DashboardHideInactive.legacy_field_id(),
                    "Hide Inactive",
                    self.dashboard_hide_inactive,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::DashboardHideInactive, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row,
                    right_x,
                    FieldRef::DashboardHideInactive,
                ));
            }
            WindowDef::ActiveEffects { .. } => {
                self.render_textarea_compact(
                    FieldRef::ActiveEffectsCategory.legacy_field_id(),
                    "Category:",
                    &self.active_effects_category_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::ActiveEffectsCategory, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::ActiveEffectsCategory));
            }
            WindowDef::Performance { .. } => {
                self.render_button(
                    FieldRef::EditMetrics.legacy_field_id(),
                    "[ Edit Metrics ]",
                    left_x,
                    special_row,
                    buf,
                    theme,
                    is_focus(FieldRef::EditMetrics, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::EditMetrics));
            }
            WindowDef::Perception { .. } => {
                // Only sort_direction is configurable (stream="percWindow", buffer_size=100 hardcoded)
                // Window clears on each update (<clearStream/>) so buffer size is irrelevant
                self.render_dropdown_compact(
                    FieldRef::PerceptionSortDirection.legacy_field_id(),
                    "Sort:",
                    self.perception_sort_direction_input
                        .lines()
                        .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or("descending"),
                    left_x,
                    special_row,
                    12,
                    buf,
                    theme,
                    is_focus(FieldRef::PerceptionSortDirection, self.focused_field),
                );
                self.render_checkbox_compact(
                    FieldRef::PerceptionUseShortSpellNames.legacy_field_id(),
                    "Short Spell Names:",
                    self.perception_use_short_spell_names,
                    right_x,
                    special_row,
                    20,
                    buf,
                    theme,
                    is_focus(FieldRef::PerceptionUseShortSpellNames, self.focused_field),
                );
                self.render_button(
                    FieldRef::PerceptionTextReplacements.legacy_field_id(),
                    "[ Edit Replacements ]",
                    left_x,
                    special_row + 1,
                    buf,
                    theme,
                    is_focus(FieldRef::PerceptionTextReplacements, self.focused_field),
                );
            }
            WindowDef::GS4Experience { .. } => {
                // Row 1: Show toggles
                self.render_checkbox_compact(
                    FieldRef::GS4ExpShowLevel.legacy_field_id(),
                    "Show Level",
                    self.gs4_exp_show_level,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpShowLevel, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::GS4ExpShowLevel));
                self.render_checkbox_compact(
                    FieldRef::GS4ExpShowExpBar.legacy_field_id(),
                    "Show Exp Bar",
                    self.gs4_exp_show_exp_bar,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpShowExpBar, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, right_x, FieldRef::GS4ExpShowExpBar));
                // Row 2: Mind bar + Total exp toggles
                self.render_checkbox_compact(
                    FieldRef::GS4ExpShowMindBar.legacy_field_id(),
                    "Show Mind Bar",
                    self.gs4_exp_show_mind_bar,
                    left_x,
                    special_row + 1,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpShowMindBar, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row + 1, left_x, FieldRef::GS4ExpShowMindBar));
                self.render_checkbox_compact(
                    FieldRef::GS4ExpShowTotalExp.legacy_field_id(),
                    "Show Total Exp",
                    self.gs4_exp_show_total_exp,
                    right_x,
                    special_row + 1,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpShowTotalExp, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 1,
                    right_x,
                    FieldRef::GS4ExpShowTotalExp,
                ));
                // Row 3: Ascension exp toggle
                self.render_checkbox_compact(
                    FieldRef::GS4ExpShowAscensionExp.legacy_field_id(),
                    "Show Ascension Exp",
                    self.gs4_exp_show_ascension_exp,
                    left_x,
                    special_row + 2,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpShowAscensionExp, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 2,
                    left_x,
                    FieldRef::GS4ExpShowAscensionExp,
                ));
                // Row 4: Bar colors
                self.render_color_field(
                    FieldRef::GS4ExpMindBarColor.legacy_field_id(),
                    "Mind",
                    &self.gs4_exp_mind_bar_color_input,
                    left_x,
                    special_row + 3,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpMindBarColor, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 3,
                    left_x,
                    FieldRef::GS4ExpMindBarColor,
                ));
                self.render_color_field(
                    FieldRef::GS4ExpExpBarColor.legacy_field_id(),
                    "Exp",
                    &self.gs4_exp_exp_bar_color_input,
                    right_x,
                    special_row + 3,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpExpBarColor, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 3,
                    right_x,
                    FieldRef::GS4ExpExpBarColor,
                ));
            }
            WindowDef::Encumbrance { .. } => {
                // Row 1: Show label checkbox
                self.render_checkbox_compact(
                    FieldRef::EncumShowLabel.legacy_field_id(),
                    "Show Label",
                    self.show_label_encum,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumShowLabel, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::EncumShowLabel));
                // Row 2: Light (0-20) and Moderate (21-50) colors
                self.render_color_field(
                    FieldRef::EncumColorLight.legacy_field_id(),
                    "Light",
                    &self.encum_color_light_input,
                    left_x,
                    special_row + 1,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorLight, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row + 1, left_x, FieldRef::EncumColorLight));
                self.render_color_field(
                    FieldRef::EncumColorModerate.legacy_field_id(),
                    "Moderate",
                    &self.encum_color_moderate_input,
                    right_x,
                    special_row + 1,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorModerate, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 1,
                    right_x,
                    FieldRef::EncumColorModerate,
                ));
                // Row 3: Heavy (51-80) and Critical (81-100) colors
                self.render_color_field(
                    FieldRef::EncumColorHeavy.legacy_field_id(),
                    "Heavy",
                    &self.encum_color_heavy_input,
                    left_x,
                    special_row + 2,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorHeavy, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row + 2, left_x, FieldRef::EncumColorHeavy));
                self.render_color_field(
                    FieldRef::EncumColorCritical.legacy_field_id(),
                    "Critical",
                    &self.encum_color_critical_input,
                    right_x,
                    special_row + 2,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorCritical, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 2,
                    right_x,
                    FieldRef::EncumColorCritical,
                ));
            }
            WindowDef::MiniVitals { .. } => {
                // Row 1: Display mode checkboxes
                self.render_checkbox_compact(
                    FieldRef::MiniVitalsNumbersOnly.legacy_field_id(),
                    "Numbers Only",
                    self.minivitals_numbers_only,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::MiniVitalsNumbersOnly, self.focused_field),
                );
                self.field_click_areas
                    .push((special_row, left_x, FieldRef::MiniVitalsNumbersOnly));
                self.render_checkbox_compact(
                    FieldRef::MiniVitalsCurrentOnly.legacy_field_id(),
                    "Current Only",
                    self.minivitals_current_only,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::MiniVitalsCurrentOnly, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row,
                    right_x,
                    FieldRef::MiniVitalsCurrentOnly,
                ));
                // Row 2: Bar order and colors editor button (handles all 5 bars)
                self.render_button(
                    FieldRef::MiniVitalsEditBarOrder.legacy_field_id(),
                    "[ Edit Bars & Colors ]",
                    left_x,
                    special_row + 1,
                    buf,
                    theme,
                    is_focus(FieldRef::MiniVitalsEditBarOrder, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 1,
                    left_x,
                    FieldRef::MiniVitalsEditBarOrder,
                ));
                // Depleted (unfilled) cell color; empty = window background
                self.render_color_field(
                    FieldRef::MiniVitalsDepletedColor.legacy_field_id(),
                    "Depleted",
                    &self.minivitals_depleted_color_input,
                    right_x,
                    special_row + 1,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::MiniVitalsDepletedColor, self.focused_field),
                );
                self.field_click_areas.push((
                    special_row + 1,
                    right_x,
                    FieldRef::MiniVitalsDepletedColor,
                ));
            }
            WindowDef::Betrayer { .. } => {
                // Betrayer widget: show_items toggle and bar color
                self.render_checkbox_compact(
                    FieldRef::BetrayerShowItems.legacy_field_id(),
                    "Show Items",
                    self.betrayer_show_items,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::BetrayerShowItems, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::BetrayerBarColor.legacy_field_id(),
                    "Bar Color",
                    &self.betrayer_bar_color_input,
                    left_x,
                    special_row + 1,
                    12,
                    buf,
                    theme,
                    is_focus(FieldRef::BetrayerBarColor, self.focused_field),
                );
            }
            _ => {
                buf.set_string(
                    left_x,
                    special_row,
                    "No special fields for this widget.",
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_color)),
                );
            }
        }
    }

    /// Render a text input field (compact format for section-based layout)
    pub(super) fn render_textarea_compact(
        &self,
        _field_id: usize,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        width: usize,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));

        let label_x = x + 2;
        buf.set_string(label_x, y, label, Style::default().fg(label_color));

        let raw_value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };
        let truncated: String = raw_value.chars().take(width).collect();
        let padded = format!("{:<width$}", truncated, width = width);

        let text_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.cursor_color
        } else {
            theme.text_color
        });
        let input_x = label_x + label.len() as u16 + 1;
        buf.set_string(input_x, y, padded, Style::default().fg(text_color));
    }

    pub(super) fn render_tab_editor_input(
        &self,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        available_width: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });
        let text_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.cursor_color
        } else {
            theme.text_color
        });

        let prefix = "  ";
        let label_width: usize = 11;
        let usable_width = available_width as usize;
        let reserved = prefix.len() + label_width + 1; // space
        let input_width = usable_width.saturating_sub(reserved);

        let raw_value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };
        let truncated: String = raw_value.chars().take(input_width).collect();
        let padded_value = format!("{:<width$}", truncated, width = input_width);

        let start_x = x;
        buf.set_string(start_x, y, prefix, Style::default().fg(label_color));
        buf.set_string(
            start_x + prefix.len() as u16,
            y,
            format!("{:<width$}", label, width = label_width),
            Style::default().fg(label_color),
        );
        let input_x = start_x + prefix.len() as u16 + label_width as u16 + 1;
        buf.set_string(input_x, y, padded_value, Style::default().fg(text_color));
    }

    pub(super) fn render_tab_editor_checkbox(
        &self,
        label: &str,
        checked: bool,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });
        let prefix = "   "; // start at column 4 to align with text fields
        let checkbox = if checked { "[✓]" } else { "[ ]" };
        let start_x = x;
        buf.set_string(start_x, y, prefix, Style::default().fg(label_color));
        let checkbox_x = start_x + prefix.len() as u16;
        buf.set_string(checkbox_x, y, checkbox, Style::default().fg(label_color));
        buf.set_string(checkbox_x + 4, y, label, Style::default().fg(label_color));
    }

    /// Render a color field with preview
    pub(super) fn render_color_field(
        &self,
        _field_id: usize,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        input_width: usize,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });
        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));

        let value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };

        // Color swatch
        let swatch_x = x + 2;
        self.render_color_preview(value, swatch_x, y, buf, theme);

        // Label after swatch
        let label_x = swatch_x + 4 + 1;
        buf.set_string(label_x, y, label, Style::default().fg(label_color));

        // Input field
        let truncated: String = value.chars().take(input_width).collect();
        let padded = format!("{:<width$}", truncated, width = input_width);
        let text_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.cursor_color
        } else {
            theme.text_color
        });
        let input_x = label_x + label.len() as u16 + 1;
        buf.set_string(input_x, y, padded, Style::default().fg(text_color));
    }

    /// Render a checkbox field (compact format)
    pub(super) fn render_checkbox_compact(
        &self,
        _field_id: usize,
        label: &str,
        checked: bool,
        x: u16,
        y: u16,
        _column_width: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));

        let label_x = x + 2;
        let label_width = usize::max(14, label.len());
        let padded_label = format!("{:<width$}", label, width = label_width);
        buf.set_string(label_x, y, padded_label, Style::default().fg(label_color));

        let checkbox = if checked { "[✓]" } else { "[ ]" };
        let checkbox_x = label_x + label_width as u16 + 2;
        buf.set_string(checkbox_x, y, checkbox, Style::default().fg(label_color));
    }

    /// Render a dropdown field (compact format)
    pub(super) fn render_dropdown_compact(
        &self,
        _field_id: usize,
        label: &str,
        value: &str,
        x: u16,
        y: u16,
        width: usize,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));
        buf.set_string(x + 2, y, label, Style::default().fg(label_color));

        let display_value = format!("{} ▼", value);
        let truncated: String = display_value.chars().take(width).collect();
        let padded = format!("{:<width$}", truncated, width = width);
        let input_x = x + 2 + label.len() as u16 + 1;
        buf.set_string(
            input_x,
            y,
            &padded,
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_color)),
        );
    }

    pub(super) fn render_color_preview(
        &self,
        color_str: &str,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
    ) {
        // Use centralized mode-aware color parser
        let color = crate::frontend::tui::colors::parse_color_to_ratatui(color_str);

        buf.set_string(
            x,
            y,
            "[",
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color)),
        );
        if let Some(color) = color {
            let style = Style::default().bg(color);
            buf[(x + 1, y)].set_char(' ').set_style(style);
            buf[(x + 2, y)].set_char(' ').set_style(style);
        } else {
            buf[(x + 1, y)].set_char(' ').reset();
            buf[(x + 2, y)].set_char(' ').reset();
        }
        buf.set_string(
            x + 3,
            y,
            "]",
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color)),
        );
    }

    pub(super) fn parse_icon_char(value: &str) -> Option<char> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        let hex = trimmed
            .trim_start_matches("0x")
            .trim_start_matches("\\u{")
            .trim_start_matches("\\u")
            .trim_start_matches("\\U")
            .trim_start_matches("u+")
            .trim_start_matches("U+")
            .trim_start_matches('u')
            .trim_start_matches('U')
            .trim_end_matches('}');
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(codepoint) = u32::from_str_radix(hex, 16) {
                let mapped = match codepoint {
                    0xe231 | 0xf231 => 0x2620, // poison skull fallback
                    _ => codepoint,
                };
                if let Some(ch) = char::from_u32(mapped) {
                    return Some(ch);
                }
            }
        }

        trimmed.chars().next()
    }

    pub(super) fn render_button(
        &self,
        _field_id: usize,
        label: &str,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_focused: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_focused {
            theme.focused_label_color
        } else {
            theme.text_color
        });

        buf.set_string(
            x,
            y,
            label,
            Style::default()
                .fg(label_color)
                .add_modifier(if is_focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        );
    }
}
