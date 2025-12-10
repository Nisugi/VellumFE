use crate::config::{Config, IndicatorTemplateEntry, IndicatorTemplateStore};
use crate::frontend::{KeyCode, KeyModifiers};
use crate::theme::AppTheme;
use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Clear, Widget},
};
use tui_textarea::TextArea;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorMode {
    List,
    Form,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormField {
    Id,
    Title,
    Icon,
    ActiveColor,
    InactiveColor,
}

pub enum EditorAction {
    None,
    Close,
}

pub struct IndicatorTemplateEditor {
    popup_x: u16,
    popup_y: u16,
    popup_width: u16,
    popup_height: u16,

    templates: Vec<IndicatorTemplateEntry>,
    selected: usize,
    mode: EditorMode,
    field: FormField,
    editing_index: Option<usize>,

    id_input: TextArea<'static>,
    title_input: TextArea<'static>,
    icon_input: TextArea<'static>,
    active_input: TextArea<'static>,
    inactive_input: TextArea<'static>,

    status: String,
}

impl IndicatorTemplateEditor {
    pub fn new() -> Self {
        // Start with all known indicator templates (built-ins + any saved overrides)
        let templates = Config::list_indicator_templates();

        let mut editor = Self {
            popup_x: 0,
            popup_y: 0,
            popup_width: 70,
            popup_height: 20,
            templates,
            selected: 0,
            mode: EditorMode::List,
            field: FormField::Id,
            editing_index: None,
            id_input: Self::create_textarea(),
            title_input: Self::create_textarea(),
            icon_input: Self::create_textarea(),
            active_input: Self::create_textarea(),
            inactive_input: Self::create_textarea(),
            status: String::new(),
        };

        editor.refresh_form_from_selected();
        editor
    }

    fn create_textarea() -> TextArea<'static> {
        let mut ta = TextArea::default();
        ta.set_max_histories(0);
        ta.set_cursor_line_style(Style::default());
        ta
    }

    fn refresh_form_from_selected(&mut self) {
        self.id_input = Self::create_textarea();
        self.title_input = Self::create_textarea();
        self.icon_input = Self::create_textarea();
        self.active_input = Self::create_textarea();
        self.inactive_input = Self::create_textarea();

        if let Some(tpl) = self.templates.get(self.selected) {
            self.id_input.insert_str(tpl.id.clone());
            if let Some(title) = tpl.title.clone() {
                self.title_input.insert_str(title);
            }
            if let Some(icon) = tpl.icon.clone() {
                self.icon_input.insert_str(icon);
            }
            if let Some(active) = tpl.active_color.clone() {
                self.active_input.insert_str(active);
            }
            if let Some(inactive) = tpl.inactive_color.clone() {
                self.inactive_input.insert_str(inactive);
            }
        }
    }

    fn current_field(&mut self) -> &mut TextArea<'static> {
        match self.field {
            FormField::Id => &mut self.id_input,
            FormField::Title => &mut self.title_input,
            FormField::Icon => &mut self.icon_input,
            FormField::ActiveColor => &mut self.active_input,
            FormField::InactiveColor => &mut self.inactive_input,
        }
    }

    fn next_field(&mut self) {
        self.field = match self.field {
            FormField::Id => FormField::Title,
            FormField::Title => FormField::Icon,
            FormField::Icon => FormField::ActiveColor,
            FormField::ActiveColor => FormField::InactiveColor,
            FormField::InactiveColor => FormField::Id,
        };
    }

    fn prev_field(&mut self) {
        self.field = match self.field {
            FormField::Id => FormField::InactiveColor,
            FormField::Title => FormField::Id,
            FormField::Icon => FormField::Title,
            FormField::ActiveColor => FormField::Icon,
            FormField::InactiveColor => FormField::ActiveColor,
        };
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> EditorAction {
        match self.mode {
            EditorMode::List => match code {
                KeyCode::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                        self.refresh_form_from_selected();
                    }
                }
                KeyCode::Down => {
                    if self.selected + 1 < self.templates.len() {
                        self.selected += 1;
                        self.refresh_form_from_selected();
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    self.start_add();
                }
                KeyCode::Enter | KeyCode::Char('e') | KeyCode::Char('E') => {
                    self.start_edit();
                }
                KeyCode::Delete | KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.delete_selected();
                }
                KeyCode::Esc => return EditorAction::Close,
                KeyCode::Char('s') if modifiers.ctrl => {
                    match self.save_store() {
                        Ok(_) => {
                            self.status = "Templates saved".to_string();
                        }
                        Err(e) => {
                            self.status = format!("Failed to save: {}", e);
                        }
                    }
                }
                _ => {}
            },
            EditorMode::Form => match code {
                KeyCode::Tab => self.next_field(),
                KeyCode::BackTab => self.prev_field(),
                KeyCode::Esc => {
                    self.mode = EditorMode::List;
                    self.editing_index = None;
                    self.status.clear();
                }
                KeyCode::Enter => self.save_form(),
                KeyCode::Char('s') if modifiers.ctrl => self.save_form(),
                _ => {
                    // Delegate editing to the current field
                    let ct_code = crate::frontend::tui::crossterm_bridge::to_crossterm_keycode(code);
                    let ct_mods = crate::frontend::tui::crossterm_bridge::to_crossterm_modifiers(modifiers);
                    let key_event = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                    let input = crate::frontend::tui::textarea_bridge::to_textarea_event(key_event);
                    let _ = self.current_field().input(input);
                }
            },
        }

        EditorAction::None
    }

    pub fn handle_paste(&mut self, text: &str) {
        if matches!(self.mode, EditorMode::Form) {
            self.current_field().insert_str(text.to_string());
        }
    }

    fn start_add(&mut self) {
        self.mode = EditorMode::Form;
        self.field = FormField::Id;
        self.editing_index = None;
        self.id_input = Self::create_textarea();
        self.title_input = Self::create_textarea();
        self.icon_input = Self::create_textarea();
        self.active_input = Self::create_textarea();
        self.inactive_input = Self::create_textarea();
        self.status.clear();
    }

    fn start_edit(&mut self) {
        if self.templates.is_empty() {
            return;
        }
        self.mode = EditorMode::Form;
        self.field = FormField::Id;
        self.editing_index = Some(self.selected);
        self.refresh_form_from_selected();
    }

    fn delete_selected(&mut self) {
        if self.templates.is_empty() {
            return;
        }
        self.templates.remove(self.selected);
        if self.selected >= self.templates.len() && !self.templates.is_empty() {
            self.selected = self.templates.len() - 1;
        }
        self.refresh_form_from_selected();
        match self.save_store() {
            Ok(_) => {
                self.status = "Template deleted".to_string();
            }
            Err(e) => {
                self.status = format!("Failed to save: {}", e);
            }
        }
    }

    fn save_form(&mut self) {
        let id_raw = self
            .id_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if id_raw.is_empty() {
            self.status = "ID is required".to_string();
            return;
        }
        let id = id_raw.to_uppercase();
        let key = id.to_lowercase();
        let is_builtin = matches!(
            key.as_str(),
            "poisoned" | "bleeding" | "diseased" | "stunned" | "webbed"
        );

        let title = self
            .title_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let mut icon = self
            .icon_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if let Some(ch) = Self::parse_icon_char(&icon) {
            if Self::looks_like_hex(&icon) {
                icon = ch.to_string();
            }
        }
        let active_color = self
            .active_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let inactive_color = self
            .inactive_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let entry = IndicatorTemplateEntry {
            id: id.clone(),
            name: Some(key.clone()),
            title: if title.is_empty() { None } else { Some(title) },
            icon: if icon.is_empty() { None } else { Some(icon) },
            inactive_color: if inactive_color.is_empty() {
                None
            } else {
                Some(inactive_color)
            },
            active_color: if active_color.is_empty() {
                None
            } else {
                Some(active_color)
            },
            default_status: None,
            default_color: None,
            enabled: true,
        };

        // Remove any existing entry with the same key (so overrides replace built-ins/customs)
        let mut filtered: Vec<IndicatorTemplateEntry> = self
            .templates
            .iter()
            .enumerate()
            .filter_map(|(idx, tpl)| {
                if self
                    .editing_index
                    .map(|edit_idx| edit_idx == idx)
                    .unwrap_or(false)
                {
                    return None;
                }
                if tpl.key().eq_ignore_ascii_case(&key) {
                    return None;
                }
                Some(tpl.clone())
            })
            .collect();

        let insert_idx = self
            .editing_index
            .unwrap_or_else(|| filtered.len())
            .min(filtered.len());
        filtered.insert(insert_idx, entry);
        self.templates = filtered;
        self.selected = insert_idx;

        self.editing_index = None;
        self.mode = EditorMode::List;
        match self.save_store() {
            Ok(_) => {
                if is_builtin {
                    self.status = format!("Saved override for {}", id);
                } else {
                    self.status = "Template saved".to_string();
                }
            }
            Err(e) => {
                self.status = format!("Failed to save: {}", e);
            }
        }
        self.refresh_form_from_selected();
    }

    fn save_store(&self) -> anyhow::Result<()> {
        let store = IndicatorTemplateStore {
            indicators: self.templates.clone(),
        };
        Config::save_indicator_template_store(&store)
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &AppTheme) {
        let width = self.popup_width.min(area.width.saturating_sub(2)).max(40);
        let height = self.popup_height.min(area.height.saturating_sub(2)).max(12);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;

        self.popup_x = x;
        self.popup_y = y;
        self.popup_width = width;
        self.popup_height = height;

        let popup_area = Rect { x, y, width, height };
        Clear.render(popup_area, buf);

        // Background
        for row in 0..height {
            for col in 0..width {
                if x + col < buf.area().width && y + row < buf.area().height {
                    buf[(x + col, y + row)]
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.editor_background));
                }
            }
        }

        // Border
        let border_color = crossterm_bridge::to_ratatui_color(theme.editor_border);
        self.draw_border(
            &popup_area,
            buf,
            border_color,
            crossterm_bridge::to_ratatui_color(theme.editor_background),
        );

        // Title
        let title = " Indicator Templates ";
        let title_x = x + 2;
        if title_x < x + width {
            buf.set_string(
                title_x,
                y,
                title,
                Style::default().fg(crossterm_bridge::to_ratatui_color(theme.editor_label)),
            );
        }

        match self.mode {
            EditorMode::List => self.render_list(buf, theme),
            EditorMode::Form => self.render_form(buf, theme),
        }

        // Status/footer
        let footer = match self.mode {
            EditorMode::List => "[A:Add] [E:Edit] [Del:Delete] [Ctrl+S: Save] [Esc: Back]",
            EditorMode::Form => "[Tab:Next] [Shift+Tab:Prev] [Ctrl+S: Save] [Esc: Cancel]",
        };
        let footer_y = y + height - 1;
        let footer_fg = border_color;
        if footer_y < buf.area().height && width >= 2 {
            let content_width = width.saturating_sub(2) as usize;
            let mut inner = footer.to_string();
            if inner.len() < content_width {
                inner.push_str(&"─".repeat(content_width - inner.len()));
            } else {
                inner.truncate(content_width);
            }
            let line = format!("└{}┘", inner);
            buf.set_string(x, footer_y, line, Style::default().fg(footer_fg));
        }

        if !self.status.is_empty() {
            let status_y = y + height.saturating_sub(2);
            if status_y < buf.area().height {
                buf.set_string(
                    x + 1,
                    status_y,
                    self.status.clone(),
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.editor_status)),
                );
            }
        }
    }

    fn render_list(&mut self, buf: &mut Buffer, theme: &AppTheme) {
        let list_x = self.popup_x + 1;
        let list_y = self.popup_y + 1;
        let list_width = self.popup_width.saturating_sub(2);
        let list_height = self.popup_height.saturating_sub(3);

        let normal = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.editor_label));
        let selected = Style::default()
            .fg(crossterm_bridge::to_ratatui_color(theme.editor_label_focused))
            .add_modifier(Modifier::BOLD);

        for row in 0..list_height {
            if list_y + row < buf.area().height {
                buf.set_string(
                    list_x,
                    list_y + row,
                    " ".repeat(list_width as usize),
                    Style::default()
                        .bg(crossterm_bridge::to_ratatui_color(theme.editor_background)),
                );
            }
        }

        for (idx, tpl) in self.templates.iter().enumerate() {
            if idx as u16 >= list_height {
                break;
            }
            let icon = if let Some(icon) = tpl.icon.as_ref() {
                if let Some(ch) = Self::parse_icon_char(icon) {
                    ch.to_string()
                } else if icon.is_empty() {
                    "?".to_string()
                } else {
                    icon.clone()
                }
            } else {
                "?".to_string()
            };
            let line = format!("{} {}", icon, tpl.id);
            let style = if idx == self.selected { selected } else { normal };
            buf.set_string(list_x, list_y + idx as u16, line, style);
        }
    }

    fn render_form(&mut self, buf: &mut Buffer, theme: &AppTheme) {
        let left_x = self.popup_x + 2;
        let mut row = self.popup_y + 2;
        let input_width = (self.popup_width.saturating_sub(6)) as usize;
        let icon_field_width = 12usize.min(input_width);
        self.render_text_field(
            "ID:",
            &self.id_input,
            row,
            left_x,
            input_width,
            matches!(self.field, FormField::Id),
            theme,
            buf,
        );
        row += 1;
        self.render_text_field(
            "Title:",
            &self.title_input,
            row,
            left_x,
            input_width,
            matches!(self.field, FormField::Title),
            theme,
            buf,
        );
        row += 1;
        self.render_text_field(
            "Icon:",
            &self.icon_input,
            row,
            left_x,
            icon_field_width,
            matches!(self.field, FormField::Icon),
            theme,
            buf,
        );
        row += 1;
        self.render_text_field(
            "Active:",
            &self.active_input,
            row,
            left_x,
            input_width / 2,
            matches!(self.field, FormField::ActiveColor),
            theme,
            buf,
        );
        row += 1;
        self.render_text_field(
            "Inactive:",
            &self.inactive_input,
            row,
            left_x,
            input_width / 2,
            matches!(self.field, FormField::InactiveColor),
            theme,
            buf,
        );

        // Icon preview when hex supplied
        if let Some(icon_char) = Self::parse_icon_char(
            self.icon_input
                .lines()
                .get(0)
                .map(|s| s.as_str())
                .unwrap_or(""),
        ) {
            let label_len = "Icon:".len() as u16;
            let preview_x = (left_x + label_len + 1 + icon_field_width as u16 + 1)
                .min(self.popup_x + self.popup_width.saturating_sub(2));
            let preview_y = self.popup_y + 4;
            if preview_x < buf.area().width && preview_y < buf.area().height {
                buf[(preview_x, preview_y)].set_char(icon_char).set_fg(
                    crossterm_bridge::to_ratatui_color(theme.editor_text),
                );
            }
        }
    }

    fn render_text_field(
        &self,
        label: &str,
        textarea: &TextArea,
        y: u16,
        x: u16,
        width: usize,
        is_current: bool,
        theme: &AppTheme,
        buf: &mut Buffer,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.editor_label_focused
        } else {
            theme.editor_label
        });
        let text_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.editor_cursor
        } else {
            theme.editor_text
        });

        buf.set_string(x, y, label, Style::default().fg(label_color));
        let raw_value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };
        let truncated: String = raw_value.chars().take(width).collect();
        let padded = format!("{:<width$}", truncated, width = width);
        buf.set_string(x + label.len() as u16 + 1, y, padded, Style::default().fg(text_color));
    }

    fn draw_border(&self, area: &Rect, buf: &mut Buffer, color: ratatui::style::Color, bg: ratatui::style::Color) {
        let Rect { x, y, width, height } = *area;
        if width < 2 || height < 2 {
            return;
        }
        for dx in 1..width - 1 {
            buf[(x + dx, y)]
                .set_char('─')
                .set_fg(color)
                .set_bg(bg);
            buf[(x + dx, y + height - 1)]
                .set_char('─')
                .set_fg(color)
                .set_bg(bg);
        }
        for dy in 1..height - 1 {
            buf[(x, y + dy)]
                .set_char('│')
                .set_fg(color)
                .set_bg(bg);
            buf[(x + width - 1, y + dy)]
                .set_char('│')
                .set_fg(color)
                .set_bg(bg);
        }
        buf[(x, y)].set_char('┌').set_fg(color).set_bg(bg);
        buf[(x + width - 1, y)].set_char('┐').set_fg(color).set_bg(bg);
        buf[(x, y + height - 1)]
            .set_char('└')
            .set_fg(color)
            .set_bg(bg);
        buf[(x + width - 1, y + height - 1)]
            .set_char('┘')
            .set_fg(color)
            .set_bg(bg);
    }

    fn parse_icon_char(value: &str) -> Option<char> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Accept common prefixes (0x, \u, \U, u+, leading u/U) and hex-only
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
                // Fallback map for known PUA aliases
                let mapped = match codepoint {
                    0xe231 | 0xf231 => 0x2620, // poison skull
                    _ => codepoint,
                };
                if let Some(ch) = char::from_u32(mapped) {
                    return Some(ch);
                }
            }
        }

        trimmed.chars().next()
    }

    fn looks_like_hex(value: &str) -> bool {
        let trimmed = value.trim();
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
        !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit())
    }
}
