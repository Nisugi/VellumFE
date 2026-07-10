//! TUI hotbar editor popup: bar list -> button list -> button form.
//!
//! Covers bar/button CRUD, button reorder, and the core button fields
//! (label, command, hotkey with validation, tooltip, category, countdown
//! source). Condition/state building is GUI-editor (or hotbars.toml)
//! territory; states authored there round-trip untouched and are shown
//! here as a count. Saves are returned to the input layer as results so
//! it can write via Config::save_hotbar and reload_hotbars().

use crate::config::{
    Config, EffectCategory, HotbarButton, HotbarCountdownSource, HotbarDef, NameMatch,
};
use crate::frontend::tui::crossterm_bridge;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Clear, Widget},
};
use tui_textarea::TextArea;

const POPUP_WIDTH: u16 = 74;
const POPUP_HEIGHT: u16 = 22;

/// Effects of a key press that the input layer must apply.
#[derive(Debug, Clone)]
pub enum HotbarEditorResult {
    None,
    Close,
    /// Write the bar to the given scope (true = global) and reload hotbars.
    SaveBar(HotbarDef, bool),
    /// Delete the named bar from every scope file it appears in.
    DeleteBar(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Bars,
    Buttons,
    Form,
}

#[derive(Clone)]
struct BarEntry {
    name: String,
    button_count: usize,
    in_global: bool,
    in_character: bool,
}

/// Which form row has focus (order matches the rendered rows).
const FORM_FIELDS: usize = 8; // label, command, hotkey, tooltip, category, cd source, cd category, cd name (+ match shares row)

struct ButtonForm {
    button_index: usize,
    label: TextArea<'static>,
    command: TextArea<'static>,
    hotkey: TextArea<'static>,
    tooltip: TextArea<'static>,
    category: TextArea<'static>,
    /// 0 = None, 1 = Roundtime, 2 = Casttime, 3 = Effect
    countdown_kind: usize,
    effect_category: EffectCategory,
    effect_name: TextArea<'static>,
    name_match: NameMatch,
    states_count: usize,
    focused: usize,
    error: Option<String>,
}

impl ButtonForm {
    fn from_button(index: usize, button: &HotbarButton) -> Self {
        let text_field = |value: &str| {
            let mut area = TextArea::default();
            area.insert_str(value);
            area
        };
        let (countdown_kind, effect_category, effect_name, name_match) = match &button.countdown {
            None => (0, EffectCategory::Cooldowns, String::new(), NameMatch::Exact),
            Some(HotbarCountdownSource::Roundtime) => {
                (1, EffectCategory::Cooldowns, String::new(), NameMatch::Exact)
            }
            Some(HotbarCountdownSource::Casttime) => {
                (2, EffectCategory::Cooldowns, String::new(), NameMatch::Exact)
            }
            Some(HotbarCountdownSource::Effect {
                category,
                name,
                name_match,
            }) => (3, *category, name.clone(), *name_match),
        };
        Self {
            button_index: index,
            label: text_field(&button.label),
            command: text_field(&button.command),
            hotkey: text_field(button.hotkey.as_deref().unwrap_or("")),
            tooltip: text_field(button.tooltip.as_deref().unwrap_or("")),
            category: text_field(button.category.as_deref().unwrap_or("")),
            countdown_kind,
            effect_category,
            effect_name: text_field(&effect_name),
            name_match,
            states_count: button.states.len(),
            focused: 0,
            error: None,
        }
    }

    fn line(area: &TextArea) -> String {
        area.lines().first().cloned().unwrap_or_default()
    }

    /// Validate and write the form's fields back onto the button.
    fn apply(&self, button: &mut HotbarButton) -> Result<(), String> {
        let label = Self::line(&self.label).trim().to_string();
        if label.is_empty() {
            return Err("Label is required".to_string());
        }
        let command = Self::line(&self.command).trim().to_string();
        if command.is_empty() {
            return Err("Command is required".to_string());
        }
        let hotkey = Self::line(&self.hotkey).trim().to_lowercase();
        if !hotkey.is_empty() && crate::config::parse_key_string(&hotkey).is_none() {
            return Err(format!("Unrecognized key combo '{}'", hotkey));
        }
        let countdown = match self.countdown_kind {
            0 => None,
            1 => Some(HotbarCountdownSource::Roundtime),
            2 => Some(HotbarCountdownSource::Casttime),
            _ => {
                let name = Self::line(&self.effect_name).trim().to_string();
                if name.is_empty() {
                    return Err("Effect name is required for an effect countdown".to_string());
                }
                Some(HotbarCountdownSource::Effect {
                    category: self.effect_category,
                    name,
                    name_match: self.name_match,
                })
            }
        };

        button.label = label;
        button.command = command;
        button.hotkey = (!hotkey.is_empty()).then_some(hotkey);
        let tooltip = Self::line(&self.tooltip).trim().to_string();
        button.tooltip = (!tooltip.is_empty()).then_some(tooltip);
        let category = Self::line(&self.category).trim().to_string();
        button.category = (!category.is_empty()).then_some(category);
        button.countdown = countdown;
        Ok(())
    }

    fn focused_textarea(&mut self) -> Option<&mut TextArea<'static>> {
        match self.focused {
            0 => Some(&mut self.label),
            1 => Some(&mut self.command),
            2 => Some(&mut self.hotkey),
            3 => Some(&mut self.tooltip),
            4 => Some(&mut self.category),
            7 if self.countdown_kind == 3 => Some(&mut self.effect_name),
            _ => None,
        }
    }

    fn cycle(&mut self, backward: bool) {
        match self.focused {
            5 => {
                self.countdown_kind = if backward {
                    (self.countdown_kind + 3) % 4
                } else {
                    (self.countdown_kind + 1) % 4
                };
            }
            6 if self.countdown_kind == 3 => {
                let all = EffectCategory::ALL;
                let idx = all
                    .iter()
                    .position(|c| *c == self.effect_category)
                    .unwrap_or(0);
                let next = if backward {
                    (idx + all.len() - 1) % all.len()
                } else {
                    (idx + 1) % all.len()
                };
                self.effect_category = all[next];
            }
            7 if self.countdown_kind == 3 => {
                // Left/Right on the name row toggles the match mode
                self.name_match = match self.name_match {
                    NameMatch::Exact => NameMatch::Contains,
                    NameMatch::Contains => NameMatch::Exact,
                };
            }
            _ => {}
        }
    }
}

pub struct HotbarEditor {
    level: Level,
    bars: Vec<BarEntry>,
    selected_bar: usize,
    new_bar_name: TextArea<'static>,
    naming_bar: bool,

    working: Option<HotbarDef>,
    is_global: bool,
    dirty: bool,
    selected_button: usize,
    form: Option<ButtonForm>,

    status: String,
    popup_x: u16,
    popup_y: u16,
}

impl HotbarEditor {
    pub fn new(config: &Config) -> Self {
        let mut editor = Self {
            level: Level::Bars,
            bars: Vec::new(),
            selected_bar: 0,
            new_bar_name: TextArea::default(),
            naming_bar: false,
            working: None,
            is_global: true,
            dirty: false,
            selected_button: 0,
            form: None,
            status: String::new(),
            popup_x: 0,
            popup_y: 0,
        };
        editor.refresh_bars(config);
        editor
    }

    /// Rebuild the bar list from config (call after saves/deletes/reloads).
    pub fn refresh_bars(&mut self, config: &Config) {
        let character = config.character.as_deref();
        self.bars = config
            .hotbars
            .bars
            .iter()
            .map(|bar| {
                let (in_global, in_character) = Config::hotbar_scope(&bar.name, character);
                BarEntry {
                    name: bar.name.clone(),
                    button_count: bar.buttons.len(),
                    in_global,
                    in_character,
                }
            })
            .collect();
        if self.selected_bar >= self.bars.len() {
            self.selected_bar = self.bars.len().saturating_sub(1);
        }
    }

    /// Handle a key press. Text-input keys go to the focused field; the rest
    /// drive navigation. Returns the side effect for the input layer.
    pub fn handle_key(&mut self, key: KeyEvent, config: &Config) -> HotbarEditorResult {
        match self.level {
            Level::Bars => self.handle_bars_key(key, config),
            Level::Buttons => self.handle_buttons_key(key),
            Level::Form => self.handle_form_key(key),
        }
    }

    fn handle_bars_key(&mut self, key: KeyEvent, config: &Config) -> HotbarEditorResult {
        if self.naming_bar {
            match key.code {
                KeyCode::Esc => {
                    self.naming_bar = false;
                    self.new_bar_name = TextArea::default();
                }
                KeyCode::Enter => {
                    let name = ButtonForm::line(&self.new_bar_name).trim().to_string();
                    if name.is_empty() {
                        self.status = "Bar name is required".to_string();
                    } else if self.bars.iter().any(|b| b.name == name) {
                        self.status = format!("Bar '{}' already exists", name);
                    } else {
                        self.naming_bar = false;
                        self.new_bar_name = TextArea::default();
                        self.working = Some(HotbarDef {
                            name,
                            title: None,
                            buttons: Vec::new(),
                        });
                        self.is_global = true;
                        self.dirty = true;
                        self.selected_button = 0;
                        self.level = Level::Buttons;
                        self.status = "New bar - Ctrl+S to save".to_string();
                    }
                }
                _ => {
                    let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key);
                    self.new_bar_name.input(rt_key);
                }
            }
            return HotbarEditorResult::None;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return HotbarEditorResult::Close,
            KeyCode::Up => self.selected_bar = self.selected_bar.saturating_sub(1),
            KeyCode::Down => {
                if self.selected_bar + 1 < self.bars.len() {
                    self.selected_bar += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = self.bars.get(self.selected_bar) {
                    if let Some(bar) = config.hotbars.find_bar(&entry.name) {
                        self.working = Some(bar.clone());
                        // Character scope wins when the bar exists there
                        self.is_global = !entry.in_character;
                        self.dirty = false;
                        self.selected_button = 0;
                        self.level = Level::Buttons;
                        self.status.clear();
                    }
                }
            }
            KeyCode::Char('a') => {
                self.naming_bar = true;
                self.status.clear();
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(entry) = self.bars.get(self.selected_bar) {
                    return HotbarEditorResult::DeleteBar(entry.name.clone());
                }
            }
            _ => {}
        }
        HotbarEditorResult::None
    }

    fn handle_buttons_key(&mut self, key: KeyEvent) -> HotbarEditorResult {
        let Some(working) = &mut self.working else {
            self.level = Level::Bars;
            return HotbarEditorResult::None;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Esc => {
                if self.dirty {
                    // First Esc warns; a save or another Esc proceeds
                    if self.status.starts_with("Unsaved") {
                        self.working = None;
                        self.dirty = false;
                        self.level = Level::Bars;
                        self.status.clear();
                    } else {
                        self.status =
                            "Unsaved changes - Ctrl+S to save, Esc again to discard".to_string();
                    }
                } else {
                    self.working = None;
                    self.level = Level::Bars;
                    self.status.clear();
                }
            }
            KeyCode::Up if ctrl => {
                if self.selected_button > 0 && !working.buttons.is_empty() {
                    working
                        .buttons
                        .swap(self.selected_button, self.selected_button - 1);
                    self.selected_button -= 1;
                    self.dirty = true;
                }
            }
            KeyCode::Down if ctrl => {
                if self.selected_button + 1 < working.buttons.len() {
                    working
                        .buttons
                        .swap(self.selected_button, self.selected_button + 1);
                    self.selected_button += 1;
                    self.dirty = true;
                }
            }
            KeyCode::Up => self.selected_button = self.selected_button.saturating_sub(1),
            KeyCode::Down => {
                if self.selected_button + 1 < working.buttons.len() {
                    self.selected_button += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                if let Some(button) = working.buttons.get(self.selected_button) {
                    self.form = Some(ButtonForm::from_button(self.selected_button, button));
                    self.level = Level::Form;
                    self.status.clear();
                }
            }
            KeyCode::Char('a') => {
                let n = working.buttons.len() + 1;
                let mut id = format!("button{}", n);
                while working.buttons.iter().any(|b| b.id == id) {
                    id.push('x');
                }
                working.buttons.push(HotbarButton {
                    id,
                    label: "New".to_string(),
                    command: String::new(),
                    hotkey: None,
                    tooltip: None,
                    category: None,
                    countdown: None,
                    states: Vec::new(),
                    default_style: None,
                });
                self.selected_button = working.buttons.len() - 1;
                self.form = Some(ButtonForm::from_button(
                    self.selected_button,
                    &working.buttons[self.selected_button],
                ));
                self.level = Level::Form;
                self.dirty = true;
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.selected_button < working.buttons.len() {
                    working.buttons.remove(self.selected_button);
                    if self.selected_button >= working.buttons.len() {
                        self.selected_button = working.buttons.len().saturating_sub(1);
                    }
                    self.dirty = true;
                }
            }
            KeyCode::Char('g') => {
                self.is_global = !self.is_global;
                self.dirty = true;
            }
            KeyCode::Char('s') if ctrl => {
                self.dirty = false;
                self.status = "Saved".to_string();
                return HotbarEditorResult::SaveBar(working.clone(), self.is_global);
            }
            _ => {}
        }
        HotbarEditorResult::None
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> HotbarEditorResult {
        let Some(form) = &mut self.form else {
            self.level = Level::Buttons;
            return HotbarEditorResult::None;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Esc => {
                self.form = None;
                self.level = Level::Buttons;
            }
            KeyCode::Tab | KeyCode::Down => {
                form.focused = (form.focused + 1) % FORM_FIELDS;
            }
            KeyCode::BackTab | KeyCode::Up => {
                form.focused = (form.focused + FORM_FIELDS - 1) % FORM_FIELDS;
            }
            KeyCode::Left if form.focused >= 5 => form.cycle(true),
            KeyCode::Right if form.focused >= 5 => form.cycle(false),
            KeyCode::Enter | KeyCode::Char('s') if key.code == KeyCode::Enter || ctrl => {
                if let Some(working) = &mut self.working {
                    if let Some(button) = working.buttons.get_mut(form.button_index) {
                        match form.apply(button) {
                            Ok(()) => {
                                self.form = None;
                                self.level = Level::Buttons;
                                self.dirty = true;
                                self.status =
                                    "Button updated - Ctrl+S to save the bar".to_string();
                            }
                            Err(err) => form.error = Some(err),
                        }
                    }
                }
            }
            _ => {
                if let Some(area) = form.focused_textarea() {
                    let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key);
                    area.input(rt_key);
                    form.error = None;
                }
            }
        }
        HotbarEditorResult::None
    }

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _config: &Config,
        theme: &crate::theme::AppTheme,
    ) {
        let width = POPUP_WIDTH.min(area.width);
        let height = POPUP_HEIGHT.min(area.height);
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(width)) / 2;
            self.popup_y = (area.height.saturating_sub(height)) / 2;
        }
        let x = self.popup_x.min(area.width.saturating_sub(width));
        let y = self.popup_y.min(area.height.saturating_sub(height));

        let popup = Rect {
            x,
            y,
            width,
            height,
        };
        Clear.render(popup, buf);
        for row in 0..height {
            for col in 0..width {
                buf[(x + col, y + row)]
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
        }
        self.draw_border(x, y, width, height, buf, theme);

        let title = match self.level {
            Level::Bars => " Hotbars ".to_string(),
            Level::Buttons => {
                let name = self.working.as_ref().map(|w| w.name.as_str()).unwrap_or("");
                let scope = if self.is_global { "[G]" } else { "[C]" };
                let dirty = if self.dirty { "*" } else { "" };
                format!(" Hotbar: {} {}{} ", name, scope, dirty)
            }
            Level::Form => " Edit Button ".to_string(),
        };
        self.put_str(
            buf,
            x + 1,
            y,
            &title,
            crossterm_bridge::to_ratatui_color(theme.form_label),
            theme,
        );

        match self.level {
            Level::Bars => self.render_bars(x, y, width, height, buf, theme),
            Level::Buttons => self.render_buttons(x, y, width, height, buf, theme),
            Level::Form => self.render_form(x, y, width, height, buf, theme),
        }

        // Status + footer
        if !self.status.is_empty() {
            let status = self.status.clone();
            self.put_str(
                buf,
                x + 2,
                y + height - 3,
                &status,
                crossterm_bridge::to_ratatui_color(theme.form_label_focused),
                theme,
            );
        }
        let footer = match self.level {
            Level::Bars => {
                if self.naming_bar {
                    "Type name  Enter:Create  Esc:Cancel"
                } else {
                    "Enter:Open  A:Add  D:Delete  Esc:Close"
                }
            }
            Level::Buttons => {
                "Enter:Edit A:Add D:Del Ctrl+Up/Dn:Reorder G:Scope Ctrl+S:Save Esc:Back"
            }
            Level::Form => "Tab/Up/Dn:Field  Left/Right:Cycle  Enter:Apply  Esc:Back",
        };
        self.put_str(
            buf,
            x + 2,
            y + height - 2,
            footer,
            crossterm_bridge::to_ratatui_color(theme.text_primary),
            theme,
        );
    }

    fn render_bars(
        &mut self,
        x: u16,
        y: u16,
        _width: u16,
        height: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let list_height = (height - 5) as usize;
        if self.naming_bar {
            self.put_str(
                buf,
                x + 2,
                y + 2,
                "New bar name:",
                crossterm_bridge::to_ratatui_color(theme.form_label),
                theme,
            );
            let name = ButtonForm::line(&self.new_bar_name);
            let display = format!("{}_", name);
            self.put_str(
                buf,
                x + 16,
                y + 2,
                &display,
                crossterm_bridge::to_ratatui_color(theme.form_label_focused),
                theme,
            );
            return;
        }
        if self.bars.is_empty() {
            self.put_str(
                buf,
                x + 2,
                y + 2,
                "No bars defined - press A to add one",
                crossterm_bridge::to_ratatui_color(theme.text_disabled),
                theme,
            );
            return;
        }
        let start = self.selected_bar.saturating_sub(list_height.saturating_sub(1));
        let entries: Vec<(usize, String)> = self
            .bars
            .iter()
            .enumerate()
            .skip(start)
            .take(list_height)
            .map(|(idx, entry)| {
                let scope = match (entry.in_global, entry.in_character) {
                    (_, true) => "[C]",
                    (true, false) => "[G]",
                    _ => "[?]",
                };
                (
                    idx,
                    format!(
                        "{} {}  ({} buttons)",
                        scope, entry.name, entry.button_count
                    ),
                )
            })
            .collect();
        for (row, (idx, text)) in entries.iter().enumerate() {
            let selected = *idx == self.selected_bar;
            let color = crossterm_bridge::to_ratatui_color(if selected {
                theme.browser_item_focused
            } else {
                theme.browser_item_normal
            });
            let text = text.clone();
            self.put_str(buf, x + 2, y + 1 + row as u16, &text, color, theme);
        }
    }

    fn render_buttons(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let Some(working) = &self.working else {
            return;
        };
        if working.buttons.is_empty() {
            self.put_str(
                buf,
                x + 2,
                y + 2,
                "No buttons - press A to add one",
                crossterm_bridge::to_ratatui_color(theme.text_disabled),
                theme,
            );
            return;
        }
        let list_height = (height - 5) as usize;
        let start = self
            .selected_button
            .saturating_sub(list_height.saturating_sub(1));
        let max_width = (width - 4) as usize;
        let rows: Vec<(usize, String)> = working
            .buttons
            .iter()
            .enumerate()
            .skip(start)
            .take(list_height)
            .map(|(idx, button)| {
                let hotkey = button
                    .hotkey
                    .as_deref()
                    .map(|k| format!(" [{}]", k))
                    .unwrap_or_default();
                let states = if button.states.is_empty() {
                    String::new()
                } else {
                    format!("  {{{} states}}", button.states.len())
                };
                let mut text = format!(
                    "{}  ({}){}{}",
                    button.label, button.command, hotkey, states
                );
                if text.len() > max_width {
                    text.truncate(max_width.saturating_sub(3));
                    text.push_str("...");
                }
                (idx, text)
            })
            .collect();
        for (row, (idx, text)) in rows.iter().enumerate() {
            let selected = *idx == self.selected_button;
            let color = crossterm_bridge::to_ratatui_color(if selected {
                theme.browser_item_focused
            } else {
                theme.browser_item_normal
            });
            let text = text.clone();
            self.put_str(buf, x + 2, y + 1 + row as u16, &text, color, theme);
        }
    }

    fn render_form(
        &mut self,
        x: u16,
        y: u16,
        _width: u16,
        _height: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let Some(form) = &self.form else {
            return;
        };

        let rows: Vec<(usize, &str, String)> = vec![
            (0, "Label:", ButtonForm::line(&form.label)),
            (1, "Command:", ButtonForm::line(&form.command)),
            (2, "Hotkey:", ButtonForm::line(&form.hotkey)),
            (3, "Tooltip:", ButtonForm::line(&form.tooltip)),
            (4, "Category:", ButtonForm::line(&form.category)),
            (
                5,
                "Countdown:",
                ["None", "Roundtime", "Casttime", "Effect"][form.countdown_kind].to_string(),
            ),
        ];

        let mut current_y = y + 2;
        for (field, label, value) in rows {
            let focused = form.focused == field;
            self.render_field_row(x, current_y, label, &value, focused, field >= 5, buf, theme);
            current_y += 2;
        }

        if form.countdown_kind == 3 {
            let focused_cat = form.focused == 6;
            let cat_value = match form.effect_category {
                EffectCategory::Buffs => "Buffs",
                EffectCategory::Debuffs => "Debuffs",
                EffectCategory::Cooldowns => "Cooldowns",
                EffectCategory::ActiveSpells => "Active Spells",
            };
            self.render_field_row(
                x, current_y, "Eff cat:", cat_value, focused_cat, true, buf, theme,
            );
            current_y += 2;

            let focused_name = form.focused == 7;
            let match_label = match form.name_match {
                NameMatch::Exact => "exact",
                NameMatch::Contains => "contains",
            };
            let value = format!(
                "{}  ({}; Left/Right toggles)",
                ButtonForm::line(&form.effect_name),
                match_label
            );
            self.render_field_row(
                x, current_y, "Eff name:", &value, focused_name, false, buf, theme,
            );
            current_y += 2;
        }

        if form.states_count > 0 {
            let note = format!(
                "{} state(s) defined - edit in the GUI editor or hotbars.toml",
                form.states_count
            );
            self.put_str(
                buf,
                x + 2,
                current_y,
                &note,
                crossterm_bridge::to_ratatui_color(theme.text_disabled),
                theme,
            );
        }

        if let Some(error) = &form.error {
            let error = error.clone();
            self.put_str(
                buf,
                x + 2,
                current_y + 1,
                &error,
                crossterm_bridge::to_ratatui_color(theme.form_label_focused),
                theme,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_field_row(
        &self,
        x: u16,
        y: u16,
        label: &str,
        value: &str,
        focused: bool,
        is_cycle: bool,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.form_label
        });
        self.put_str(buf, x + 2, y, label, label_color, theme);

        let value_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else if is_cycle {
            theme.text_disabled
        } else {
            theme.text_primary
        });
        let display = if focused && !is_cycle {
            format!("{}_", value)
        } else {
            value.to_string()
        };
        self.put_str(buf, x + 13, y, &display, value_color, theme);
    }

    fn put_str(
        &self,
        buf: &mut Buffer,
        x: u16,
        y: u16,
        text: &str,
        color: ratatui::style::Color,
        theme: &crate::theme::AppTheme,
    ) {
        let max_x = (self.popup_x + POPUP_WIDTH).saturating_sub(1);
        for (i, ch) in text.chars().enumerate() {
            let cx = x + i as u16;
            if cx >= max_x || cx >= buf.area().width || y >= buf.area().height {
                break;
            }
            buf[(cx, y)]
                .set_char(ch)
                .set_fg(color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
    }

    fn draw_border(
        &self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let border_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.browser_border));
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
            buf[(x + col, y + height - 1)]
                .set_char('─')
                .set_style(border_style);
        }
        buf[(x + width - 1, y)]
            .set_char('┐')
            .set_style(border_style);
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)]
                .set_char('│')
                .set_style(border_style);
        }
        buf[(x, y + height - 1)]
            .set_char('└')
            .set_style(border_style);
        buf[(x + width - 1, y + height - 1)]
            .set_char('┘')
            .set_style(border_style);
    }
}
