use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};
use tui_textarea::TextArea;
use std::collections::HashMap;

use crate::config::{DashboardIndicatorDef, TabConfig, WindowDef};

/// Result of window editor interaction
#[derive(Debug, Clone)]
pub enum WindowEditorResult {
    Save { window: WindowDef, is_new: bool, original_name: Option<String> },
    Cancel,
}

/// Window editor widget - keybind form style
pub struct WindowEditor {
    // Mode
    pub mode: EditorMode,
    pub active: bool,

    // Window selection (for edit mode)
    available_windows: Vec<String>,
    selected_window_index: usize,

    // Widget type selection (for new window mode)
    available_widget_types: Vec<String>,
    selected_widget_type_index: usize,

    // Editing state
    is_new_window: bool,
    original_window_name: Option<String>,
    current_window: WindowDef,

    // Form fields with focused field tracking
    focused_field: usize,

    // Text input fields using TextArea
    name_input: TextArea<'static>,
    row_input: TextArea<'static>,
    col_input: TextArea<'static>,
    rows_input: TextArea<'static>,
    cols_input: TextArea<'static>,
    border_color_input: TextArea<'static>,
    title_input: TextArea<'static>,
    bg_color_input: TextArea<'static>,
    buffer_size_input: TextArea<'static>,

    // Dropdown states (just store selected index)
    border_style_index: usize,
    content_align_index: usize,
    tab_bar_position_index: usize,

    // Checkbox states
    show_border: bool,
    transparent_bg: bool,
    locked: bool,

    // Multi-checkbox state (border sides)
    border_sides_selected: usize,  // Which checkbox is highlighted
    border_sides_states: HashMap<String, bool>,

    // Status
    status_message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorMode {
    SelectingWindow,
    SelectingWidgetType,
    EditingFields,
}

// Dropdown options
const BORDER_STYLES: &[&str] = &["none", "single", "double", "rounded", "thick"];
const CONTENT_ALIGNS: &[&str] = &["top-left", "top-center", "top-right", "center-left", "center", "center-right", "bottom-left", "bottom-center", "bottom-right"];
const TAB_BAR_POSITIONS: &[&str] = &["top", "bottom"];
const BORDER_SIDES: &[&str] = &["top", "bottom", "left", "right"];

impl WindowEditor {
    pub fn new() -> Self {
        Self {
            mode: EditorMode::SelectingWindow,
            active: false,
            available_windows: Vec::new(),
            selected_window_index: 0,
            available_widget_types: vec![
                "text".to_string(),
                "tabbed".to_string(),
                "active_effects".to_string(),
                "targets".to_string(),
                "players".to_string(),
                "dashboard".to_string(),
                "indicator".to_string(),
                "compass".to_string(),
                "injury_doll".to_string(),
                "progress".to_string(),
                "countdown".to_string(),
            ],
            selected_widget_type_index: 0,
            is_new_window: false,
            original_window_name: None,
            current_window: WindowDef::default(),
            focused_field: 0,
            name_input: TextArea::default(),
            row_input: TextArea::default(),
            col_input: TextArea::default(),
            rows_input: TextArea::default(),
            cols_input: TextArea::default(),
            border_color_input: TextArea::default(),
            title_input: TextArea::default(),
            bg_color_input: TextArea::default(),
            buffer_size_input: TextArea::default(),
            border_style_index: 1, // "single"
            content_align_index: 0, // "top-left"
            tab_bar_position_index: 0, // "top"
            show_border: true,
            transparent_bg: false,
            locked: false,
            border_sides_selected: 0,
            border_sides_states: HashMap::new(),
            status_message: String::new(),
        }
    }

    /// Open editor for selecting and editing an existing window
    pub fn open_for_window(&mut self, windows: Vec<String>, selected: Option<String>) {
        self.available_windows = windows;
        self.selected_window_index = if let Some(name) = selected {
            self.available_windows.iter().position(|w| w == &name).unwrap_or(0)
        } else {
            0
        };
        self.mode = EditorMode::SelectingWindow;
        self.active = true;
        self.status_message = "↑/↓: Navigate | Enter: Edit window | Esc: Cancel".to_string();
    }

    /// Open editor for creating a new window
    pub fn open_for_new_window(&mut self) {
        self.mode = EditorMode::SelectingWidgetType;
        self.selected_widget_type_index = 0;
        self.active = true;
        self.is_new_window = true;
        self.status_message = "↑/↓: Navigate | Enter: Select widget type | Esc: Cancel".to_string();
    }

    /// Load a window for editing
    pub fn load_window(&mut self, window: WindowDef) {
        self.is_new_window = false;
        self.original_window_name = Some(window.name.clone());
        self.current_window = window.clone();
        self.populate_fields_from_window();
        self.mode = EditorMode::EditingFields;
        self.focused_field = 0;
        self.update_status();
    }

    /// Initialize a new window with widget type
    pub fn init_new_window(&mut self, widget_type: String) {
        self.is_new_window = true;
        self.original_window_name = None;
        self.current_window = WindowDef::default();
        self.current_window.widget_type = widget_type.clone();
        self.current_window.name = "new_window".to_string();

        // Apply smart defaults
        self.apply_widget_defaults(&widget_type);
        self.populate_fields_from_window();

        self.mode = EditorMode::EditingFields;
        self.focused_field = 0;
        self.update_status();
    }

    fn apply_widget_defaults(&mut self, widget_type: &str) {
        match widget_type {
            "compass" => {
                self.current_window.rows = 5;
                self.current_window.cols = 10;
            },
            "injury_doll" => {
                self.current_window.rows = 12;
                self.current_window.cols = 20;
            },
            "progress" | "countdown" => {
                self.current_window.rows = 3;
                self.current_window.cols = 20;
            },
            "indicator" => {
                self.current_window.rows = 3;
                self.current_window.cols = 15;
            },
            "dashboard" => {
                self.current_window.rows = 5;
                self.current_window.cols = 40;
            },
            "text" => {
                self.current_window.rows = 20;
                self.current_window.cols = 80;
            },
            "tabbed" => {
                self.current_window.rows = 20;
                self.current_window.cols = 60;
            },
            "targets" | "players" => {
                self.current_window.rows = 10;
                self.current_window.cols = 30;
            },
            "active_effects" => {
                self.current_window.rows = 15;
                self.current_window.cols = 35;
            },
            _ => {
                self.current_window.rows = 10;
                self.current_window.cols = 40;
            }
        }
    }

    fn populate_fields_from_window(&mut self) {
        self.name_input.delete_line_by_head();
        self.name_input.insert_str(&self.current_window.name);

        self.row_input.delete_line_by_head();
        self.row_input.insert_str(&self.current_window.row.to_string());

        self.col_input.delete_line_by_head();
        self.col_input.insert_str(&self.current_window.col.to_string());

        self.rows_input.delete_line_by_head();
        self.rows_input.insert_str(&self.current_window.rows.to_string());

        self.cols_input.delete_line_by_head();
        self.cols_input.insert_str(&self.current_window.cols.to_string());

        self.buffer_size_input.delete_line_by_head();
        self.buffer_size_input.insert_str(&self.current_window.buffer_size.to_string());

        self.border_color_input.delete_line_by_head();
        if let Some(color) = &self.current_window.border_color {
            self.border_color_input.insert_str(color);
        }

        self.title_input.delete_line_by_head();
        if let Some(title) = &self.current_window.title {
            self.title_input.insert_str(title);
        }

        self.bg_color_input.delete_line_by_head();
        if let Some(bg) = &self.current_window.background_color {
            self.bg_color_input.insert_str(bg);
        }

        // Checkboxes
        self.show_border = self.current_window.show_border;
        self.transparent_bg = self.current_window.transparent_background;
        self.locked = self.current_window.locked;

        // Dropdowns - find index
        self.border_style_index = BORDER_STYLES.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.border_style)
            .unwrap_or(1);

        self.content_align_index = CONTENT_ALIGNS.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.content_align)
            .unwrap_or(0);

        self.tab_bar_position_index = TAB_BAR_POSITIONS.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.tab_bar_position)
            .unwrap_or(0);

        // Border sides
        self.border_sides_states.clear();
        for side in BORDER_SIDES {
            let checked = if let Some(sides) = &self.current_window.border_sides {
                sides.contains(&side.to_string())
            } else {
                true // Default all checked
            };
            self.border_sides_states.insert(side.to_string(), checked);
        }
        self.border_sides_selected = 0;
    }

    fn update_status(&mut self) {
        self.status_message = "Tab/Shift+Tab: Navigate | Space: Toggle | ↑/↓: Scroll dropdown | Ctrl+S: Save | Esc: Cancel".to_string();
    }

    fn save_fields_to_window(&mut self) {
        self.current_window.name = self.name_input.lines()[0].to_string();
        self.current_window.row = self.row_input.lines()[0].parse().unwrap_or(0);
        self.current_window.col = self.col_input.lines()[0].parse().unwrap_or(0);
        self.current_window.rows = self.rows_input.lines()[0].parse::<u16>().unwrap_or(10).max(3);
        self.current_window.cols = self.cols_input.lines()[0].parse::<u16>().unwrap_or(40).max(10);
        self.current_window.buffer_size = self.buffer_size_input.lines()[0].parse::<usize>().unwrap_or(1000).max(100);

        let border_color = self.border_color_input.lines()[0].to_string();
        self.current_window.border_color = if border_color.is_empty() { None } else { Some(border_color) };

        let title = self.title_input.lines()[0].to_string();
        self.current_window.title = if title.is_empty() { None } else { Some(title) };

        let bg = self.bg_color_input.lines()[0].to_string();
        self.current_window.background_color = if bg.is_empty() { None } else { Some(bg) };

        self.current_window.show_border = self.show_border;
        self.current_window.transparent_background = self.transparent_bg;
        self.current_window.locked = self.locked;

        self.current_window.border_style = Some(BORDER_STYLES[self.border_style_index].to_string());
        self.current_window.content_align = Some(CONTENT_ALIGNS[self.content_align_index].to_string());
        self.current_window.tab_bar_position = Some(TAB_BAR_POSITIONS[self.tab_bar_position_index].to_string());

        // Border sides
        let checked_sides: Vec<String> = BORDER_SIDES.iter()
            .filter(|&&s| *self.border_sides_states.get(s).unwrap_or(&true))
            .map(|&s| s.to_string())
            .collect();
        self.current_window.border_sides = if checked_sides.len() == 4 {
            None // All sides = default
        } else {
            Some(checked_sides)
        };
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match self.mode {
            EditorMode::SelectingWindow => self.handle_window_selection_key(key),
            EditorMode::SelectingWidgetType => self.handle_widget_type_selection_key(key),
            EditorMode::EditingFields => self.handle_fields_key(key),
        }
    }

    fn handle_window_selection_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match key.code {
            KeyCode::Esc => {
                self.active = false;
                Some(WindowEditorResult::Cancel)
            },
            KeyCode::Up => {
                if self.selected_window_index > 0 {
                    self.selected_window_index -= 1;
                }
                None
            },
            KeyCode::Down => {
                if self.selected_window_index + 1 < self.available_windows.len() {
                    self.selected_window_index += 1;
                }
                None
            },
            KeyCode::Enter => {
                // Signal to load the selected window (app.rs will handle this)
                None
            },
            _ => None,
        }
    }

    fn handle_widget_type_selection_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match key.code {
            KeyCode::Esc => {
                self.active = false;
                Some(WindowEditorResult::Cancel)
            },
            KeyCode::Up => {
                if self.selected_widget_type_index > 0 {
                    self.selected_widget_type_index -= 1;
                }
                None
            },
            KeyCode::Down => {
                if self.selected_widget_type_index + 1 < self.available_widget_types.len() {
                    self.selected_widget_type_index += 1;
                }
                None
            },
            KeyCode::Enter => {
                let widget_type = self.available_widget_types[self.selected_widget_type_index].clone();
                self.init_new_window(widget_type);
                None
            },
            _ => None,
        }
    }

    fn handle_fields_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        // Field IDs: 0=name, 1=row, 2=col, 3=rows, 4=cols, 5=show_border, 6=border_style,
        // 7=border_sides, 8=border_color, 9=title, 10=content_align, 11=transparent_bg,
        // 12=bg_color, 13=locked, 14=buffer_size, 15=save, 16=cancel

        match key.code {
            KeyCode::Tab => {
                let max_field = 16;
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    if self.focused_field == 0 {
                        self.focused_field = max_field;
                    } else {
                        self.focused_field -= 1;
                    }
                } else {
                    if self.focused_field >= max_field {
                        self.focused_field = 0;
                    } else {
                        self.focused_field += 1;
                    }
                }
                None
            },
            KeyCode::BackTab => {
                // Shift+Tab (BackTab)
                let max_field = 16;
                if self.focused_field == 0 {
                    self.focused_field = max_field;
                } else {
                    self.focused_field -= 1;
                }
                None
            },
            KeyCode::Esc => {
                self.active = false;
                Some(WindowEditorResult::Cancel)
            },
            KeyCode::Char(' ') if self.focused_field == 5 => {
                self.show_border = !self.show_border;
                None
            },
            KeyCode::Char(' ') if self.focused_field == 11 => {
                self.transparent_bg = !self.transparent_bg;
                None
            },
            KeyCode::Char(' ') if self.focused_field == 13 => {
                self.locked = !self.locked;
                None
            },
            KeyCode::Char(' ') if self.focused_field == 7 => {
                // Toggle selected border side
                let side = BORDER_SIDES[self.border_sides_selected];
                if let Some(checked) = self.border_sides_states.get_mut(side) {
                    *checked = !*checked;
                }
                None
            },
            KeyCode::Up if self.focused_field == 6 => {
                self.border_style_index = self.border_style_index.saturating_sub(1);
                None
            },
            KeyCode::Down if self.focused_field == 6 => {
                self.border_style_index = (self.border_style_index + 1).min(BORDER_STYLES.len() - 1);
                None
            },
            KeyCode::Up if self.focused_field == 10 => {
                self.content_align_index = self.content_align_index.saturating_sub(1);
                None
            },
            KeyCode::Down if self.focused_field == 10 => {
                self.content_align_index = (self.content_align_index + 1).min(CONTENT_ALIGNS.len() - 1);
                None
            },
            KeyCode::Up if self.focused_field == 7 => {
                if self.border_sides_selected > 0 {
                    self.border_sides_selected -= 1;
                }
                None
            },
            KeyCode::Down if self.focused_field == 7 => {
                if self.border_sides_selected + 1 < BORDER_SIDES.len() {
                    self.border_sides_selected += 1;
                }
                None
            },
            KeyCode::Enter if self.focused_field == 15 => {
                // Save button
                self.save_fields_to_window();
                self.active = false;
                Some(WindowEditorResult::Save {
                    window: self.current_window.clone(),
                    is_new: self.is_new_window,
                    original_name: self.original_window_name.clone(),
                })
            },
            KeyCode::Enter if self.focused_field == 16 => {
                // Cancel button
                self.active = false;
                Some(WindowEditorResult::Cancel)
            },
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+S to save
                self.save_fields_to_window();
                self.active = false;
                Some(WindowEditorResult::Save {
                    window: self.current_window.clone(),
                    is_new: self.is_new_window,
                    original_name: self.original_window_name.clone(),
                })
            },
            _ => {
                // Pass to text inputs
                use tui_textarea::Input;
                let input: Input = key.into();

                let _handled = match self.focused_field {
                    0 => self.name_input.input(input.clone()),
                    1 => self.row_input.input(input.clone()),
                    2 => self.col_input.input(input.clone()),
                    3 => self.rows_input.input(input.clone()),
                    4 => self.cols_input.input(input.clone()),
                    8 => self.border_color_input.input(input.clone()),
                    9 => self.title_input.input(input.clone()),
                    12 => self.bg_color_input.input(input.clone()),
                    14 => self.buffer_size_input.input(input.clone()),
                    _ => false,
                };
                None
            }
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        RatatuiWidget::render(Clear, area, buf);

        match self.mode {
            EditorMode::SelectingWindow => self.render_window_selection(area, buf),
            EditorMode::SelectingWidgetType => self.render_widget_type_selection(area, buf),
            EditorMode::EditingFields => self.render_fields(area, buf),
        }
    }

    fn render_window_selection(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new("Select Window to Edit")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        title.render(chunks[0], buf);

        let items: Vec<Line> = self.available_windows.iter().enumerate().map(|(i, name)| {
            let style = if i == self.selected_window_index {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(format!("  {}", name), style))
        }).collect();

        let list = Paragraph::new(items).block(Block::default().borders(Borders::ALL).title("Windows"));
        list.render(chunks[1], buf);

        let status = Paragraph::new(&self.status_message as &str)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        status.render(chunks[2], buf);
    }

    fn render_widget_type_selection(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new("Select Widget Type")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        title.render(chunks[0], buf);

        let items: Vec<Line> = self.available_widget_types.iter().enumerate().map(|(i, wtype)| {
            let style = if i == self.selected_widget_type_index {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(format!("  {}", wtype), style))
        }).collect();

        let list = Paragraph::new(items).block(Block::default().borders(Borders::ALL).title("Widget Types"));
        list.render(chunks[1], buf);

        let status = Paragraph::new(&self.status_message as &str)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        status.render(chunks[2], buf);
    }

    fn render_fields(&mut self, area: Rect, buf: &mut Buffer) {
        let popup_width = 100.min(area.width);
        let popup_height = 40.min(area.height);  // Increased from 30 to 40
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(if self.is_new_window { " Add Window " } else { " Edit Window " })
            .style(Style::default().bg(Color::Black));
        block.render(popup_area, buf);

        let content = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(2),
        };

        let mut y = content.y;

        // Title
        let title = if self.is_new_window {
            format!("Create new {} window", self.current_window.widget_type)
        } else {
            format!("Edit window: {}", self.current_window.name)
        };
        let title_para = Paragraph::new(title).style(Style::default().add_modifier(Modifier::BOLD));
        title_para.render(Rect { x: content.x, y, width: content.width, height: 1 }, buf);
        y += 2;

        // Name field
        Self::render_text_field(0, self.focused_field, "Name:", &mut self.name_input, content.x, y, content.width, buf);
        y += 3;

        // Position fields (2 columns)
        let col_width = content.width / 2;
        Self::render_text_field(1, self.focused_field, "Row:", &mut self.row_input, content.x, y, col_width - 1, buf);
        Self::render_text_field(2, self.focused_field, "Col:", &mut self.col_input, content.x + col_width, y, col_width, buf);
        y += 3;

        // Size fields (2 columns)
        Self::render_text_field(3, self.focused_field, "Height:", &mut self.rows_input, content.x, y, col_width - 1, buf);
        Self::render_text_field(4, self.focused_field, "Width:", &mut self.cols_input, content.x + col_width, y, col_width, buf);
        y += 3;

        // Border section
        Self::render_checkbox(5, self.focused_field, "Show Border:", self.show_border, content.x, y, buf);
        y += 2;

        Self::render_dropdown(6, self.focused_field, "Border Style:", BORDER_STYLES[self.border_style_index], self.border_style_index, BORDER_STYLES.len(), content.x, y, content.width, buf);
        y += 2;

        Self::render_multi_checkbox(7, self.focused_field, "Border Sides:", self.border_sides_selected, &self.border_sides_states, content.x, y, buf);
        y += 2;

        Self::render_text_field(8, self.focused_field, "Border Color:", &mut self.border_color_input, content.x, y, content.width, buf);
        y += 3;

        // Other fields
        Self::render_text_field(9, self.focused_field, "Title:", &mut self.title_input, content.x, y, content.width, buf);
        y += 3;

        Self::render_dropdown(10, self.focused_field, "Content Align:", CONTENT_ALIGNS[self.content_align_index], self.content_align_index, CONTENT_ALIGNS.len(), content.x, y, content.width, buf);
        y += 2;

        Self::render_checkbox(11, self.focused_field, "Transparent BG:", self.transparent_bg, content.x, y, buf);
        y += 2;

        Self::render_text_field(12, self.focused_field, "BG Color:", &mut self.bg_color_input, content.x, y, content.width, buf);
        y += 3;

        Self::render_checkbox(13, self.focused_field, "Lock Window:", self.locked, content.x, y, buf);
        y += 2;

        // Buffer size only for text/tabbed windows
        if matches!(self.current_window.widget_type.as_str(), "text" | "tabbed") {
            Self::render_text_field(14, self.focused_field, "Buffer Size:", &mut self.buffer_size_input, content.x, y, content.width, buf);
            y += 3;
        }

        // Tabbed window configuration
        if self.current_window.widget_type == "tabbed" {
            let tab_count = self.current_window.tabs.as_ref().map(|t| t.len()).unwrap_or(0);
            let tab_info = format!("Tabs: {} configured (use .addtab/.removetab commands)", tab_count);
            let tab_para = Paragraph::new(tab_info).style(Style::default().fg(Color::DarkGray));
            tab_para.render(Rect { x: content.x, y, width: content.width, height: 1 }, buf);
            y += 2;
        }

        // Buttons
        Self::render_buttons(self.focused_field, content.x, y, buf);
        y += 2;

        // Status
        let status = Paragraph::new(&self.status_message as &str)
            .style(Style::default().fg(Color::DarkGray));
        status.render(Rect { x: content.x, y, width: content.width, height: 1 }, buf);
    }

    fn render_text_field(field_id: usize, focused_field: usize, label: &str, textarea: &mut TextArea, x: u16, y: u16, width: u16, buf: &mut Buffer) {
        let label_para = Paragraph::new(label);
        label_para.render(Rect { x, y, width: 15, height: 1 }, buf);

        let input_area = Rect {
            x: x + 15,
            y,
            width: width.saturating_sub(15),
            height: 3,
        };

        let border_style = if focused_field == field_id {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        textarea.set_block(Block::default().borders(Borders::ALL).border_style(border_style));
        textarea.set_cursor_line_style(Style::default());
        textarea.render(input_area, buf);
    }

    fn render_checkbox(field_id: usize, focused_field: usize, label: &str, checked: bool, x: u16, y: u16, buf: &mut Buffer) {
        let label_para = Paragraph::new(label);
        label_para.render(Rect { x, y, width: 20, height: 1 }, buf);

        let checkbox_text = if checked { "[X]" } else { "[ ]" };
        let style = if focused_field == field_id {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(if checked { Color::Green } else { Color::DarkGray })
        };
        let checkbox_para = Paragraph::new(checkbox_text).style(style);
        checkbox_para.render(Rect { x: x + 20, y, width: 10, height: 1 }, buf);
    }

    fn render_dropdown(field_id: usize, focused_field: usize, label: &str, value: &str, index: usize, total: usize, x: u16, y: u16, width: u16, buf: &mut Buffer) {
        let label_para = Paragraph::new(label);
        label_para.render(Rect { x, y, width: 20, height: 1 }, buf);

        let value_text = format!("{} ({}/{})", value, index + 1, total);
        let style = if focused_field == field_id {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let value_para = Paragraph::new(value_text).style(style);
        value_para.render(Rect { x: x + 20, y, width: width.saturating_sub(20), height: 1 }, buf);
    }

    fn render_multi_checkbox(field_id: usize, focused_field: usize, label: &str, border_sides_selected: usize, border_sides_states: &HashMap<String, bool>, x: u16, y: u16, buf: &mut Buffer) {
        let label_para = Paragraph::new(label);
        label_para.render(Rect { x, y, width: 20, height: 1 }, buf);

        let items: Vec<String> = BORDER_SIDES.iter().enumerate().map(|(i, &side)| {
            let checked = *border_sides_states.get(side).unwrap_or(&true);
            let checkbox = if checked { "[X]" } else { "[ ]" };
            let is_selected = i == border_sides_selected && focused_field == field_id;

            if is_selected {
                format!("→ {} {}", checkbox, side)
            } else {
                format!("  {} {}", checkbox, side)
            }
        }).collect();

        let style = if focused_field == field_id {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let items_text = items.join(" ");
        let items_para = Paragraph::new(items_text).style(style);
        items_para.render(Rect { x: x + 20, y, width: 60, height: 1 }, buf);
    }

    fn render_buttons(focused_field: usize, x: u16, y: u16, buf: &mut Buffer) {
        let save_style = if focused_field == 15 {
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let save_btn = Paragraph::new("[ Save ]").style(save_style);
        save_btn.render(Rect { x, y, width: 10, height: 1 }, buf);

        let cancel_style = if focused_field == 16 {
            Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        let cancel_btn = Paragraph::new("[ Cancel ]").style(cancel_style);
        cancel_btn.render(Rect { x: x + 12, y, width: 12, height: 1 }, buf);
    }

    pub fn get_selected_window_name(&self) -> Option<String> {
        if self.mode == EditorMode::SelectingWindow && self.selected_window_index < self.available_windows.len() {
            Some(self.available_windows[self.selected_window_index].clone())
        } else {
            None
        }
    }

    pub fn close(&mut self) {
        self.active = false;
        self.mode = EditorMode::SelectingWindow;
    }
}
