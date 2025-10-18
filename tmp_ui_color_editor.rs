use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget},
};
use tui_textarea::TextArea;
use crate::config::{PresetColor, PromptColor};

#[derive(Debug, Clone)]
pub struct UiGlobalColors {
    pub command_echo_color: String,
    pub default_border_color: String,
    pub focused_border_color: String,
    pub default_text_color: String,
    pub selection_bg_color: String,
}

#[derive(Debug, Clone)]
pub enum UiColorEditorResult {
    Save {
        globals: UiGlobalColors,
        presets: Vec<(String, PresetColor)>,
        prompts: Vec<PromptColor>,
    },
    Cancel,
}

struct PresetEntry {
    name: String,
    fg: TextArea<'static>,
    bg: TextArea<'static>,
}

struct PromptEntry {
    ch: String,
    fg: TextArea<'static>,
    bg: TextArea<'static>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section { Globals, Presets, Prompts }

pub struct UiColorEditorWidget {
    // Sections
    section: Section,
    // Globals
    cmd_echo: TextArea<'static>,
    def_border: TextArea<'static>,
    foc_border: TextArea<'static>,
    def_text: TextArea<'static>,
    sel_bg: TextArea<'static>,
    globals_focus: usize, // 0..=4

    // Presets
    presets: Vec<PresetEntry>,
    preset_index: usize,
    preset_focus: usize, // 0=fg,1=bg

    // Prompts
    prompts: Vec<PromptEntry>,
    prompt_index: usize,
    prompt_focus: usize, // 0=fg,1=bg

    // Popup
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset: (u16, u16),
}

impl UiColorEditorWidget {
    pub fn new_from_config(cfg: &crate::config::Config) -> Self {
        let mut ta = |s: &str| {
            let mut t = TextArea::default();
            if !s.is_empty() { t.insert_str(s); }
            t
        };

        // Globals
        let cmd_echo = ta(&cfg.ui.command_echo_color);
        let def_border = ta(&cfg.ui.default_border_color);
        let foc_border = ta(&cfg.ui.focused_border_color);
        let def_text = ta(&cfg.ui.default_text_color);
        let sel_bg = ta(&cfg.ui.selection_bg_color);

        // Presets sorted by name
        let mut preset_names: Vec<String> = cfg.presets.keys().cloned().collect();
        preset_names.sort();
        let presets = preset_names.into_iter().map(|name| {
            let p = cfg.presets.get(&name).cloned().unwrap_or(PresetColor{ fg: None, bg: None });
            let mut fg = TextArea::default();
            if let Some(ref v) = p.fg { fg.insert_str(v); }
            let mut bg = TextArea::default();
            if let Some(ref v) = p.bg { bg.insert_str(v); }
            PresetEntry { name, fg, bg }
        }).collect();

        // Prompts as-is
        let prompts = cfg.ui.prompt_colors.iter().map(|pc| {
            let mut fg = TextArea::default();
            if let Some(ref v) = pc.fg.as_ref().or(pc.color.as_ref()) { fg.insert_str(v); }
            let mut bg = TextArea::default();
            if let Some(ref v) = pc.bg { bg.insert_str(v); }
            PromptEntry { ch: pc.character.clone(), fg, bg }
        }).collect();

        Self {
            section: Section::Globals,
            cmd_echo,
            def_border,
            foc_border,
            def_text,
            sel_bg,
            globals_focus: 0,
            presets,
            preset_index: 0,
            preset_focus: 0,
            prompts,
            prompt_index: 0,
            prompt_focus: 0,
            popup_x: 6,
            popup_y: 2,
            is_dragging: false,
            drag_offset: (0, 0),
        }
    }

    pub fn input(&mut self, key: ratatui::crossterm::event::KeyEvent) -> Option<UiColorEditorResult> {
        match key.code {
            KeyCode::Esc => return Some(UiColorEditorResult::Cancel),
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(self.save_result());
            }
            KeyCode::Tab => {
                // Cycle sections
                self.section = match self.section { Section::Globals => Section::Presets, Section::Presets => Section::Prompts, Section::Prompts => Section::Globals };
                return None;
            }
            KeyCode::BackTab => {
                self.section = match self.section { Section::Globals => Section::Prompts, Section::Presets => Section::Globals, Section::Prompts => Section::Presets };
                return None;
            }
            _ => {}
        }

        // Convert to tui_textarea Input
        use tui_textarea::Input;
        let input: Input = key.into();

        match self.section {
            Section::Globals => {
                let focus = self.globals_focus;
                match key.code {
                    KeyCode::Up => { if self.globals_focus > 0 { self.globals_focus -= 1; } }
                    KeyCode::Down | KeyCode::Enter => { if self.globals_focus < 4 { self.globals_focus += 1; } }
                    _ => {
                        match focus {
                            0 => { self.cmd_echo.input(input); }
                            1 => { self.def_border.input(input); }
                            2 => { self.foc_border.input(input); }
                            3 => { self.def_text.input(input); }
                            4 => { self.sel_bg.input(input); }
                            _ => {}
                        }
                    }
                }
            }
            Section::Presets => {
                match key.code {
                    KeyCode::Up => { if self.preset_index > 0 { self.preset_index -= 1; } }
                    KeyCode::Down => { if self.preset_index + 1 < self.presets.len() { self.preset_index += 1; } }
                    KeyCode::Left => { if self.preset_focus > 0 { self.preset_focus -= 1; } }
                    KeyCode::Right | KeyCode::Enter => { if self.preset_focus < 1 { self.preset_focus += 1; } }
                    _ => {
                        if let Some(entry) = self.presets.get_mut(self.preset_index) {
                            if self.preset_focus == 0 { let _ = entry.fg.input(input.clone()); } else { let _ = entry.bg.input(input.clone()); }
                        }
                    }
                }
            }
            Section::Prompts => {
                match key.code {
                    KeyCode::Up => { if self.prompt_index > 0 { self.prompt_index -= 1; } }
                    KeyCode::Down => { if self.prompt_index + 1 < self.prompts.len() { self.prompt_index += 1; } }
                    KeyCode::Left => { if self.prompt_focus > 0 { self.prompt_focus -= 1; } }
                    KeyCode::Right | KeyCode::Enter => { if self.prompt_focus < 1 { self.prompt_focus += 1; } }
                    _ => {
                        if let Some(entry) = self.prompts.get_mut(self.prompt_index) {
                            if self.prompt_focus == 0 { let _ = entry.fg.input(input.clone()); } else { let _ = entry.bg.input(input.clone()); }
                        }
                    }
                }
            }
        }

        None
    }

    fn save_result(&self) -> UiColorEditorResult {
        // Globals
        let globals = UiGlobalColors {
            command_echo_color: self.cmd_echo.lines()[0].to_string(),
            default_border_color: self.def_border.lines()[0].to_string(),
            focused_border_color: self.foc_border.lines()[0].to_string(),
            default_text_color: self.def_text.lines()[0].to_string(),
            selection_bg_color: self.sel_bg.lines()[0].to_string(),
        };

        // Presets
        let presets: Vec<(String, PresetColor)> = self.presets.iter().map(|e| {
            let fg = {
                let s = e.fg.lines()[0].to_string();
                if s.trim().is_empty() || s.trim() == "-" { None } else { Some(s) }
            };
            let bg = {
                let s = e.bg.lines()[0].to_string();
                if s.trim().is_empty() || s.trim() == "-" { None } else { Some(s) }
            };
            (e.name.clone(), PresetColor { fg, bg })
        }).collect();

        // Prompts
        let prompts: Vec<PromptColor> = self.prompts.iter().map(|p| {
            let fg = {
                let s = p.fg.lines()[0].to_string();
                if s.trim().is_empty() || s.trim() == "-" { None } else { Some(s) }
            };
            let bg = {
                let s = p.bg.lines()[0].to_string();
                if s.trim().is_empty() || s.trim() == "-" { None } else { Some(s) }
            };
            PromptColor { character: p.ch.clone(), fg, bg, color: None }
        }).collect();

        UiColorEditorResult::Save { globals, presets, prompts }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let popup_width = 70;
        let popup_height = 20;

        // Draw background
        for y in self.popup_y..self.popup_y + popup_height { for x in self.popup_x..self.popup_x + popup_width { if x < area.width && y < area.height { buf.set_string(x, y, " ", Style::default().bg(Color::Black)); } } }

        // Border and title
        let title = " UI Color Editor ";
        let border_style = Style::default().fg(Color::Cyan);
        let top = format!("�{}�", "�".repeat(popup_width as usize - 2));
        buf.set_string(self.popup_x, self.popup_y, &top, border_style);
        buf.set_string(self.popup_x + 2, self.popup_y, title, border_style.add_modifier(Modifier::BOLD));
        for i in 1..popup_height - 1 { buf.set_string(self.popup_x, self.popup_y + i, "�", border_style); buf.set_string(self.popup_x + popup_width - 1, self.popup_y + i, "�", border_style); }
        let bottom = format!("�{}�", "�".repeat(popup_width as usize - 2));
        buf.set_string(self.popup_x, self.popup_y + popup_height - 1, &bottom, border_style);

        // Layout within
        let mut y = self.popup_y + 2;
        let x0 = self.popup_x + 2;
        // Globals section
        self.render_section_header("Globals", x0, y, buf, self.section == Section::Globals);
        y += 1;
        UiColorEditorWidget::render_kv("Command Echo:", &mut self.cmd_echo, x0, y, 28, self.section == Section::Globals && self.globals_focus == 0, buf);
        y += 1;
        UiColorEditorWidget::render_kv("Default Border:", &mut self.def_border, x0, y, 28, self.section == Section::Globals && self.globals_focus == 1, buf);
        y += 1;
        UiColorEditorWidget::render_kv("Focused Border:", &mut self.foc_border, x0, y, 28, self.section == Section::Globals && self.globals_focus == 2, buf);
        y += 1;
        UiColorEditorWidget::render_kv("Default Text:", &mut self.def_text, x0, y, 28, self.section == Section::Globals && self.globals_focus == 3, buf);
        y += 1;
        UiColorEditorWidget::render_kv("Selection BG:", &mut self.sel_bg, x0, y, 28, self.section == Section::Globals && self.globals_focus == 4, buf);
        y += 1;

        // Presets section
        self.render_section_header("Presets", x0, y, buf, self.section == Section::Presets);
        y += 1;
        self.render_presets(x0, y, popup_width - 4, 6, buf);
        y += 7;

        // Prompts section
        self.render_section_header("Prompts", x0, y, buf, self.section == Section::Prompts);
        y += 1;
        self.render_prompts(x0, y, popup_width - 4, 6, buf);
        y += 7;

        // Footer
        let footer = "Tab: Next Section  Shift+Tab: Prev  / Move  Ctrl+S: Save  Esc: Close"; //   as arrows
        buf.set_string(x0, self.popup_y + popup_height - 2, footer, Style::default().fg(Color::Gray));
    }

    fn render_section_header(&self, text: &str, x: u16, y: u16, buf: &mut Buffer, focused: bool) {
        let style = if focused { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Rgb(100,149,237)) };
        buf.set_string(x, y, text, style);
    }

    fn render_kv(label: &str, ta: &mut TextArea, x: u16, y: u16, label_w: u16, focused: bool, buf: &mut Buffer) {
        let style = if focused { Style::default().fg(Color::Black).bg(Color::Rgb(255,215,0)).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Cyan).bg(Color::Rgb(53,5,5)) };
        let lbl_style = if focused { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Rgb(100,149,237)) };
        let label_area = Rect { x, y, width: label_w, height: 1 };
        RatatuiWidget::render(Paragraph::new(Line::from(Span::styled(label, lbl_style))), label_area, buf);
        ta.set_style(style);
        ta.set_placeholder_style(Style::default().fg(Color::Gray).bg(Color::Rgb(53,5,5)));
        let area = Rect { x: x + label_w, y, width: 20, height: 1 };
        RatatuiWidget::render(&*ta, area, buf);
        // preview
        let val = ta.lines()[0].to_string();
        if let Some(c) = UiColorEditorWidget::parse_hex_color(&val) { buf.set_string(area.x + area.width + 1, y, "    ", Style::default().bg(c)); }
    }

    fn render_presets(&mut self, x: u16, y: u16, width: u16, rows: u16, buf: &mut Buffer) {
        let start = self.preset_index.saturating_sub(0);
        let end = (start + rows as usize).min(self.presets.len());
        for (i, idx) in (start..end).enumerate() {
            let entry = &mut self.presets[idx];
            let row_y = y + i as u16;
            let selected = idx == self.preset_index;
            let name_style = if selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) };
            buf.set_string(x, row_y, format!("{:>14}: ", entry.name), name_style);
            // FG
            let mut style = Style::default().fg(Color::Cyan).bg(Color::Rgb(53,5,5));
            if selected && self.preset_focus == 0 { style = Style::default().fg(Color::Black).bg(Color::Rgb(255,215,0)).add_modifier(Modifier::BOLD); }
            entry.fg.set_style(style);
            RatatuiWidget::render(&entry.fg, Rect{ x: x + 16, y: row_y, width: 10, height:1 }, buf);
            if let Some(c) = UiColorEditorWidget::parse_hex_color(&entry.fg.lines()[0]) { buf.set_string(x + 27, row_y, "    ", Style::default().bg(c)); }
            // BG
            let mut style2 = Style::default().fg(Color::Cyan).bg(Color::Rgb(53,5,5));
            if selected && self.preset_focus == 1 { style2 = Style::default().fg(Color::Black).bg(Color::Rgb(255,215,0)).add_modifier(Modifier::BOLD); }
            entry.bg.set_style(style2);
            RatatuiWidget::render(&entry.bg, Rect{ x: x + 32, y: row_y, width: 10, height:1 }, buf);
            if let Some(c) = UiColorEditorWidget::parse_hex_color(&entry.bg.lines()[0]) { buf.set_string(x + 43, row_y, "    ", Style::default().bg(c)); }
        }
    }

    fn render_prompts(&mut self, x: u16, y: u16, _width: u16, rows: u16, buf: &mut Buffer) {
        let start = self.prompt_index.saturating_sub(0);
        let end = (start + rows as usize).min(self.prompts.len());
        for (i, idx) in (start..end).enumerate() {
            let entry = &mut self.prompts[idx];
            let row_y = y + i as u16;
            let selected = idx == self.prompt_index;
            let name_style = if selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) };
            buf.set_string(x, row_y, format!("Prompt {:>2}: ", entry.ch), name_style);
            // FG
            let mut style = Style::default().fg(Color::Cyan).bg(Color::Rgb(53,5,5));
            if selected && self.prompt_focus == 0 { style = Style::default().fg(Color::Black).bg(Color::Rgb(255,215,0)).add_modifier(Modifier::BOLD); }
            entry.fg.set_style(style);
            RatatuiWidget::render(&entry.fg, Rect{ x: x + 13, y: row_y, width: 10, height:1 }, buf);
            if let Some(c) = UiColorEditorWidget::parse_hex_color(&entry.fg.lines()[0]) { buf.set_string(x + 24, row_y, "    ", Style::default().bg(c)); }
            // BG
            let mut style2 = Style::default().fg(Color::Cyan).bg(Color::Rgb(53,5,5));
            if selected && self.prompt_focus == 1 { style2 = Style::default().fg(Color::Black).bg(Color::Rgb(255,215,0)).add_modifier(Modifier::BOLD); }
            entry.bg.set_style(style2);
            RatatuiWidget::render(&entry.bg, Rect{ x: x + 30, y: row_y, width: 10, height:1 }, buf);
            if let Some(c) = UiColorEditorWidget::parse_hex_color(&entry.bg.lines()[0]) { buf.set_string(x + 41, row_y, "    ", Style::default().bg(c)); }
        }
    }

    fn parse_hex_color(s: &str) -> Option<Color> {
        let s = s.trim();
        if !s.starts_with('#') || s.len() < 7 { return None; }
        let s = &s[1..7];
        let (r,g,b) = (
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        );
        Some(Color::Rgb(r,g,b))
    }
}









