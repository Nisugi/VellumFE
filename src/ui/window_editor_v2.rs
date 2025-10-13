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

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,

    // Window selection (for edit mode)
    available_windows: Vec<String>,
    selected_window_index: usize,

    // Widget type selection (for new window mode)
    available_widget_types: Vec<String>,
    selected_widget_type_index: usize,

    // Template selection (after widget type is chosen)
    available_templates: Vec<String>,
    selected_template_index: usize,
    template_selected: bool,

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
    text_color_input: TextArea<'static>,
    buffer_size_input: TextArea<'static>,
    streams_input: TextArea<'static>,
    hand_icon_input: TextArea<'static>,
    countdown_icon_input: TextArea<'static>,
    bar_color_input: TextArea<'static>,
    compass_active_color_input: TextArea<'static>,
    compass_inactive_color_input: TextArea<'static>,

    // Dropdown states (just store selected index)
    border_style_index: usize,
    content_align_index: usize,
    tab_bar_position_index: usize,
    effect_category_index: usize,

    // Checkbox states
    show_border: bool,
    transparent_bg: bool,
    locked: bool,
    show_title: bool,  // If false, saves title as Some("") to hide title bar

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
    SelectingTemplate,
    EditingFields,
}

// Dropdown options
const BORDER_STYLES: &[&str] = &["none", "single", "double", "rounded", "thick", "quadrant_inside", "quadrant_outside"];
const CONTENT_ALIGNS: &[&str] = &["top-left", "top-center", "top-right", "center-left", "center", "center-right", "bottom-left", "bottom-center", "bottom-right"];
const TAB_BAR_POSITIONS: &[&str] = &["top", "bottom"];
const BORDER_SIDES: &[&str] = &["top", "bottom", "left", "right"];
const EFFECT_CATEGORIES: &[&str] = &["ActiveSpells", "Buffs", "Debuffs", "Cooldowns", "All"];

/// Get available templates for a widget type
fn get_templates_for_widget_type(widget_type: &str) -> Vec<&'static str> {
    match widget_type {
        "text" => vec!["thoughts", "speech", "familiar", "room", "logons", "deaths", "arrivals", "ambients", "announcements", "loot", "custom"],
        "tabbed" => vec!["custom"],
        "progress" => vec!["health", "mana", "stamina", "spirit", "bloodpoints", "stance", "encumbrance", "mindstate", "custom"],
        "countdown" => vec!["roundtime", "casttime", "stuntime", "custom"],
        "active_effects" => vec!["active_spells", "buffs", "debuffs", "cooldowns", "all_effects", "custom"],
        "entity" => vec!["targets", "players", "custom"],
        "dashboard" => vec!["status_dashboard", "custom"],
        "indicator" => vec!["poisoned", "diseased", "bleeding", "stunned", "webbed", "custom"],
        "compass" => vec!["compass"],
        "injury_doll" => vec!["injuries"],
        "hands" => vec!["hands", "lefthand", "righthand", "spellhand"],
        _ => vec!["custom"],
    }
}

impl WindowEditor {
    pub fn new() -> Self {
        Self {
            mode: EditorMode::SelectingWindow,
            active: false,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            available_windows: Vec::new(),
            selected_window_index: 0,
            available_widget_types: vec![
                "text".to_string(),
                "tabbed".to_string(),
                "progress".to_string(),
                "countdown".to_string(),
                "active_effects".to_string(),
                "entity".to_string(),
                "dashboard".to_string(),
                "indicator".to_string(),
                "compass".to_string(),
                "injury_doll".to_string(),
                "hands".to_string(),
            ],
            selected_widget_type_index: 0,
            available_templates: Vec::new(),
            selected_template_index: 0,
            template_selected: false,
            is_new_window: false,
            original_window_name: None,
            current_window: WindowDef::default(),
            focused_field: 0,
            name_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., my_window");
                ta
            },
            row_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., 0");
                ta
            },
            col_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., 0");
                ta
            },
            rows_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., 20");
                ta
            },
            cols_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., 80");
                ta
            },
            border_color_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., #00ff00 or green");
                ta
            },
            title_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., My Window Title");
                ta
            },
            bg_color_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., #000000 or black");
                ta
            },
            text_color_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., #ffffff or white");
                ta
            },
            buffer_size_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., 5000");
                ta
            },
            streams_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., main, speech, thoughts");
                ta
            },
            hand_icon_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., ✋");
                ta
            },
            countdown_icon_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., ⏱");
                ta
            },
            bar_color_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., #0000ff or blue");
                ta
            },
            compass_active_color_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., #00ff00 or green");
                ta
            },
            compass_inactive_color_input: {
                let mut ta = TextArea::default();
                ta.set_cursor_line_style(Style::default());
                ta.set_placeholder_text("e.g., #333333 or gray");
                ta
            },
            border_style_index: 1, // "single"
            content_align_index: 0, // "top-left"
            tab_bar_position_index: 0, // "top"
            effect_category_index: 4, // "All"
            show_border: true,
            transparent_bg: false,
            locked: false,
            show_title: true,
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
        self.active = true;
        // Set initial focused field based on widget type
        self.focused_field = if window.widget_type == "command_input" {
            9  // Title field (first field for command_input)
        } else {
            0  // Name field (first field for normal windows)
        };
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

    /// Load a template into the editor
    pub fn load_template(&mut self, template_name: &str) {
        use crate::config::Config;

        self.is_new_window = true;
        self.original_window_name = None;

        // Load template from config
        if let Some(template) = Config::get_window_template(template_name) {
            self.current_window = template;
            // Make the name unique for new windows
            self.current_window.name = format!("{}_new", template_name);
        } else {
            // Fallback to default if template not found
            self.current_window = WindowDef::default();
            self.current_window.name = format!("{}_new", template_name);
        }

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
            "entity" => {
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
        self.name_input.set_placeholder_text("e.g., my_window");
        self.name_input.insert_str(&self.current_window.name);

        self.row_input.delete_line_by_head();
        self.row_input.set_placeholder_text("e.g., 0");
        self.row_input.insert_str(&self.current_window.row.to_string());

        self.col_input.delete_line_by_head();
        self.col_input.set_placeholder_text("e.g., 0");
        self.col_input.insert_str(&self.current_window.col.to_string());

        self.rows_input.delete_line_by_head();
        self.rows_input.set_placeholder_text("e.g., 20");
        self.rows_input.insert_str(&self.current_window.rows.to_string());

        self.cols_input.delete_line_by_head();
        self.cols_input.set_placeholder_text("e.g., 80");
        self.cols_input.insert_str(&self.current_window.cols.to_string());

        self.buffer_size_input.delete_line_by_head();
        self.buffer_size_input.set_placeholder_text("e.g., 5000");
        self.buffer_size_input.insert_str(&self.current_window.buffer_size.to_string());

        self.border_color_input.delete_line_by_head();
        self.border_color_input.set_placeholder_text("e.g., #00ff00 or green");
        if let Some(color) = &self.current_window.border_color {
            self.border_color_input.insert_str(color);
        }

        self.title_input.delete_line_by_head();
        self.title_input.set_placeholder_text("e.g., My Window Title");
        if let Some(title) = &self.current_window.title {
            self.title_input.insert_str(title);
        }

        self.bg_color_input.delete_line_by_head();
        self.bg_color_input.set_placeholder_text("e.g., #000000 or black");
        if let Some(bg) = &self.current_window.background_color {
            self.bg_color_input.insert_str(bg);
        }

        if let Some(text_color) = &self.current_window.text_color {
            self.text_color_input.insert_str(text_color);
        }

        self.streams_input.delete_line_by_head();
        self.streams_input.set_placeholder_text("e.g., main, speech, thoughts");
        self.streams_input.insert_str(&self.current_window.streams.join(", "));

        self.hand_icon_input.delete_line_by_head();
        self.hand_icon_input.set_placeholder_text("e.g., ✋");
        if let Some(ref icon) = self.current_window.hand_icon {
            self.hand_icon_input.insert_str(icon);
        }

        self.countdown_icon_input.delete_line_by_head();
        self.countdown_icon_input.set_placeholder_text("(empty = default \u{f0c8})");
        if let Some(ref icon) = self.current_window.countdown_icon {
            self.countdown_icon_input.insert_str(icon);
        }

        self.bar_color_input.delete_line_by_head();
        self.bar_color_input.set_placeholder_text("e.g., #0000ff or blue");
        if let Some(ref color) = self.current_window.bar_color {
            self.bar_color_input.insert_str(color);
        }

        self.compass_active_color_input.delete_line_by_head();
        self.compass_active_color_input.set_placeholder_text("e.g., #00ff00 or green");
        if let Some(ref color) = self.current_window.compass_active_color {
            self.compass_active_color_input.insert_str(color);
        }

        self.compass_inactive_color_input.delete_line_by_head();
        self.compass_inactive_color_input.set_placeholder_text("e.g., #333333 or gray");
        if let Some(ref color) = self.current_window.compass_inactive_color {
            self.compass_inactive_color_input.insert_str(color);
        }

        // Checkboxes
        self.show_border = self.current_window.show_border;
        self.transparent_bg = self.current_window.transparent_background;
        self.locked = self.current_window.locked;

        // Show title checkbox: false if title is explicitly Some(""), true otherwise
        self.show_title = !matches!(&self.current_window.title, Some(t) if t.is_empty());

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

        self.effect_category_index = EFFECT_CATEGORIES.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.effect_category)
            .unwrap_or(4); // Default to "All"

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
        self.current_window.rows = self.rows_input.lines()[0].parse::<u16>().unwrap_or(10).max(1);
        self.current_window.cols = self.cols_input.lines()[0].parse::<u16>().unwrap_or(40).max(1);
        self.current_window.buffer_size = self.buffer_size_input.lines()[0].parse::<usize>().unwrap_or(1000).max(100);

        let border_color = self.border_color_input.lines()[0].to_string();
        self.current_window.border_color = if border_color.is_empty() { None } else { Some(border_color) };

        let title = self.title_input.lines()[0].to_string();
        self.current_window.title = if !self.show_title {
            // Show title unchecked = explicitly hide title
            Some("".to_string())
        } else if title.is_empty() {
            // Show title checked + empty = use name as title
            None
        } else {
            // Show title checked + text = use custom title
            Some(title)
        };

        let bg = self.bg_color_input.lines()[0].to_string();
        self.current_window.background_color = if bg.is_empty() { None } else { Some(bg) };

        let text_color = self.text_color_input.lines()[0].to_string();
        self.current_window.text_color = if text_color.is_empty() { None } else { Some(text_color) };

        // Parse streams from comma-separated input
        let streams_text = self.streams_input.lines()[0].to_string();
        self.current_window.streams = if streams_text.trim().is_empty() {
            Vec::new()
        } else {
            streams_text
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        // Hand icon
        let hand_icon = self.hand_icon_input.lines()[0].to_string();
        self.current_window.hand_icon = if hand_icon.is_empty() { None } else { Some(hand_icon) };

        // Countdown icon
        let countdown_icon = self.countdown_icon_input.lines()[0].to_string();
        self.current_window.countdown_icon = if countdown_icon.is_empty() { None } else { Some(countdown_icon) };

        // Bar color (for countdown and progress bars)
        let bar_color = self.bar_color_input.lines()[0].to_string();
        self.current_window.bar_color = if bar_color.is_empty() { None } else { Some(bar_color) };

        // Compass colors
        let compass_active_color = self.compass_active_color_input.lines()[0].to_string();
        self.current_window.compass_active_color = if compass_active_color.is_empty() { None } else { Some(compass_active_color) };

        let compass_inactive_color = self.compass_inactive_color_input.lines()[0].to_string();
        self.current_window.compass_inactive_color = if compass_inactive_color.is_empty() { None } else { Some(compass_inactive_color) };

        self.current_window.show_border = self.show_border;
        self.current_window.transparent_background = self.transparent_bg;
        self.current_window.locked = self.locked;

        self.current_window.border_style = Some(BORDER_STYLES[self.border_style_index].to_string());
        self.current_window.content_align = Some(CONTENT_ALIGNS[self.content_align_index].to_string());
        self.current_window.tab_bar_position = Some(TAB_BAR_POSITIONS[self.tab_bar_position_index].to_string());
        self.current_window.effect_category = Some(EFFECT_CATEGORIES[self.effect_category_index].to_string());

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
            EditorMode::SelectingTemplate => self.handle_template_selection_key(key),
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
                // Load templates for this widget type
                self.available_templates = get_templates_for_widget_type(&widget_type)
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                self.selected_template_index = 0;
                self.template_selected = false;
                self.mode = EditorMode::SelectingTemplate;
                self.status_message = "↑/↓: Navigate | Enter: Select template | Esc: Back".to_string();
                None
            },
            _ => None,
        }
    }

    fn handle_template_selection_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match key.code {
            KeyCode::Esc => {
                // Go back to widget type selection
                self.mode = EditorMode::SelectingWidgetType;
                self.status_message = "↑/↓: Navigate | Enter: Select widget type | Esc: Cancel".to_string();
                None
            },
            KeyCode::Up => {
                if self.selected_template_index > 0 {
                    self.selected_template_index -= 1;
                }
                None
            },
            KeyCode::Down => {
                if self.selected_template_index + 1 < self.available_templates.len() {
                    self.selected_template_index += 1;
                }
                None
            },
            KeyCode::Enter => {
                let widget_type = self.available_widget_types[self.selected_widget_type_index].clone();
                let template_name = self.available_templates[self.selected_template_index].clone();

                // Load template (or create custom window)
                if template_name == "custom" {
                    self.init_new_window(widget_type);
                } else {
                    self.load_template(&template_name);
                }
                self.template_selected = true;
                None
            },
            _ => None,
        }
    }

    fn handle_fields_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        // Field IDs: 0=name, 1=row, 2=col, 3=rows, 4=cols, 5=show_border, 6=border_style,
        // 7=border_sides, 8=border_color, 9=title, 10=content_align, 11=transparent_bg,
        // 12=bg_color, 13=locked, 14=streams, 15=hand_icon, 16=buffer_size, 17=save, 18=cancel, 19=effect_category, 20=show_title

        match key.code {
            KeyCode::Tab => {
                // Dynamic tab order that skips hidden fields
                let tab_order = self.get_tab_order();

                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Backward
                    if let Some(pos) = tab_order.iter().position(|&f| f == self.focused_field) {
                        if pos == 0 {
                            self.focused_field = tab_order[tab_order.len() - 1];
                        } else {
                            self.focused_field = tab_order[pos - 1];
                        }
                    }
                } else {
                    // Forward
                    if let Some(pos) = tab_order.iter().position(|&f| f == self.focused_field) {
                        if pos >= tab_order.len() - 1 {
                            self.focused_field = tab_order[0];
                        } else {
                            self.focused_field = tab_order[pos + 1];
                        }
                    } else {
                        // Focused field not in tab order, reset to first field
                        self.focused_field = tab_order[0];
                    }
                }
                None
            },
            KeyCode::BackTab => {
                // Shift+Tab (BackTab) - same as Tab with Shift
                let tab_order = self.get_tab_order();
                if let Some(pos) = tab_order.iter().position(|&f| f == self.focused_field) {
                    if pos == 0 {
                        self.focused_field = tab_order[tab_order.len() - 1];
                    } else {
                        self.focused_field = tab_order[pos - 1];
                    }
                } else {
                    // Focused field not in tab order, reset to first field
                    self.focused_field = tab_order[0];
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
            KeyCode::Char(' ') if self.focused_field == 20 => {
                self.show_title = !self.show_title;
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
            KeyCode::Up if self.focused_field == 19 => {
                self.effect_category_index = self.effect_category_index.saturating_sub(1);
                None
            },
            KeyCode::Down if self.focused_field == 19 => {
                self.effect_category_index = (self.effect_category_index + 1).min(EFFECT_CATEGORIES.len() - 1);
                None
            },
            KeyCode::Left if self.focused_field == 7 => {
                if self.border_sides_selected > 0 {
                    self.border_sides_selected -= 1;
                }
                None
            },
            KeyCode::Right if self.focused_field == 7 => {
                if self.border_sides_selected + 1 < BORDER_SIDES.len() {
                    self.border_sides_selected += 1;
                }
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
            KeyCode::Enter if self.focused_field == 17 => {
                // Save button
                self.save_fields_to_window();
                self.active = false;
                Some(WindowEditorResult::Save {
                    window: self.current_window.clone(),
                    is_new: self.is_new_window,
                    original_name: self.original_window_name.clone(),
                })
            },
            KeyCode::Enter if self.focused_field == 18 => {
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
                    14 => self.streams_input.input(input.clone()),
                    15 => self.hand_icon_input.input(input.clone()),
                    16 => self.buffer_size_input.input(input.clone()),
                    21 => self.countdown_icon_input.input(input.clone()),
                    22 => self.bar_color_input.input(input.clone()),
                    23 => self.text_color_input.input(input.clone()),
                    24 => self.compass_active_color_input.input(input.clone()),
                    25 => self.compass_inactive_color_input.input(input.clone()),
                    _ => false,
                };
                None
            }
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        // Don't clear - render as popup on top of existing windows

        match self.mode {
            EditorMode::SelectingWindow => self.render_window_selection(area, buf),
            EditorMode::SelectingWidgetType => self.render_widget_type_selection(area, buf),
            EditorMode::SelectingTemplate => self.render_template_selection(area, buf),
            EditorMode::EditingFields => self.render_fields(area, buf, config),
        }
    }

    fn render_window_selection(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 60.min(area.width);
        let popup_height = 20.min(area.height);
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Fill background with solid black
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    buf.get_mut(x, y).set_char(' ').set_bg(Color::Black);
                }
            }
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(popup_area);

        let title = Paragraph::new("Select Window to Edit")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        title.render(chunks[0], buf);

        // Calculate visible window for scrolling
        let list_height = chunks[1].height.saturating_sub(2) as usize; // Subtract borders
        let total_items = self.available_windows.len();

        // Calculate scroll offset to keep selected item visible
        let scroll_offset = if self.selected_window_index < list_height {
            0
        } else {
            self.selected_window_index.saturating_sub(list_height / 2)
        };

        let visible_end = (scroll_offset + list_height).min(total_items);

        // Only render visible items
        let items: Vec<Line> = self.available_windows[scroll_offset..visible_end]
            .iter()
            .enumerate()
            .map(|(offset_i, name)| {
                let i = scroll_offset + offset_i;
                let style = if i == self.selected_window_index {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!("  {}", name), style))
            }).collect();

        // Add scroll indicators
        let title_text = if total_items > list_height {
            format!("Windows ({}/{}) ↑↓", self.selected_window_index + 1, total_items)
        } else {
            format!("Windows ({}/{})", self.selected_window_index + 1, total_items)
        };

        let list = Paragraph::new(items).block(Block::default().borders(Borders::ALL).title(title_text));
        list.render(chunks[1], buf);

        let status = Paragraph::new(&self.status_message as &str)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        status.render(chunks[2], buf);
    }

    fn render_widget_type_selection(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 60.min(area.width);
        let popup_height = 20.min(area.height);
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Fill background with solid black
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    buf.get_mut(x, y).set_char(' ').set_bg(Color::Black);
                }
            }
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(popup_area);

        let title = Paragraph::new("Select Widget Type")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
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

    fn render_template_selection(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 60.min(area.width);
        let popup_height = 20.min(area.height);
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Fill background with solid black
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    buf.get_mut(x, y).set_char(' ').set_bg(Color::Black);
                }
            }
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(popup_area);

        let widget_type = &self.available_widget_types[self.selected_widget_type_index];
        let title_text = format!("Select {} Template", widget_type);
        let title = Paragraph::new(title_text)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        title.render(chunks[0], buf);

        let items: Vec<Line> = self.available_templates.iter().enumerate().map(|(i, template)| {
            let style = if i == self.selected_template_index {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(format!("  {}", template), style))
        }).collect();

        let list = Paragraph::new(items).block(Block::default().borders(Borders::ALL).title("Templates"));
        list.render(chunks[1], buf);

        let status = Paragraph::new(&self.status_message as &str)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        status.render(chunks[2], buf);
    }

    fn render_fields(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        let popup_width = 100.min(area.width);
        let popup_height = 50.min(area.height);

        // Use stored position if set, otherwise center
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(popup_height)) / 2;
        }

        // Clamp position to screen bounds
        self.popup_x = self.popup_x.min(area.width.saturating_sub(popup_width));
        self.popup_y = self.popup_y.min(area.height.saturating_sub(popup_height));

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Fill background with solid black
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    buf.get_mut(x, y).set_char(' ').set_bg(Color::Black);
                }
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(if self.is_new_window { " Add Window (drag title to move) " } else { " Edit Window (drag title to move) " })
            .style(Style::default().bg(Color::Black).fg(Color::Cyan));
        block.render(popup_area, buf);

        let content = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(2),
        };

        let mut y = content.y;

        // Title
        let title = if self.current_window.widget_type == "command_input" {
            "Edit Command Input Box".to_string()
        } else if self.is_new_window {
            format!("Create new {} window", self.current_window.widget_type)
        } else {
            format!("Edit window: {}", self.current_window.name)
        };
        let title_para = Paragraph::new(title).style(Style::default().add_modifier(Modifier::BOLD));
        title_para.render(Rect { x: content.x, y, width: content.width, height: 1 }, buf);
        y += 2;

        // === WINDOW IDENTITY ===
        // Skip for command_input (name is always "command_input")
        if self.current_window.widget_type != "command_input" {
            Self::render_section_header("Window Identity", content.x, y, buf);
            y += 1;

            Self::render_text_field(0, self.focused_field, "Name:", &mut self.name_input, content.x, y, content.width, buf);
            y += 3;
        }

        Self::render_text_field(9, self.focused_field, "Title:", &mut self.title_input, content.x, y, content.width, buf);
        y += 3;

        Self::render_checkbox(20, self.focused_field, "Show Title:", self.show_title, content.x, y, buf);
        y += 2;

        // === POSITION & SIZE ===
        Self::render_section_header("Position & Size", content.x, y, buf);
        y += 1;

        let col_width = content.width / 2;
        Self::render_text_field(1, self.focused_field, "Row:", &mut self.row_input, content.x, y, col_width - 1, buf);
        Self::render_text_field(2, self.focused_field, "Col:", &mut self.col_input, content.x + col_width, y, col_width, buf);
        y += 3;

        Self::render_text_field(3, self.focused_field, "Height:", &mut self.rows_input, content.x, y, col_width - 1, buf);
        Self::render_text_field(4, self.focused_field, "Width:", &mut self.cols_input, content.x + col_width, y, col_width, buf);
        y += 3;

        // === BORDER SETTINGS ===
        Self::render_section_header("Border Settings", content.x, y, buf);
        y += 1;

        // Show Border checkbox and Border Style dropdown on same line
        Self::render_checkbox(5, self.focused_field, "Show Border:", self.show_border, content.x, y, buf);
        Self::render_dropdown(6, self.focused_field, "Style:", BORDER_STYLES[self.border_style_index], self.border_style_index, BORDER_STYLES.len(), content.x + 35, y, col_width, buf);
        y += 2;

        Self::render_multi_checkbox(7, self.focused_field, "Border Sides:", self.border_sides_selected, &self.border_sides_states, content.x, y, buf);
        y += 2;

        Self::render_color_field(8, self.focused_field, "Border Color:", &mut self.border_color_input, content.x, y, content.width, buf, config);
        y += 3;

        // === DISPLAY SETTINGS ===
        Self::render_section_header("Display Settings", content.x, y, buf);
        y += 1;

        // Checkboxes on same line
        Self::render_checkbox(11, self.focused_field, "Transparent BG:", self.transparent_bg, content.x, y, buf);
        // Lock checkbox only for windows (not command_input)
        if self.current_window.widget_type != "command_input" {
            Self::render_checkbox(13, self.focused_field, "Lock:", self.locked, content.x + 40, y, buf);
        }
        y += 2;

        Self::render_color_field(12, self.focused_field, "BG Color:", &mut self.bg_color_input, content.x, y, content.width, buf, config);
        y += 3;

        // Text Color field (for hands and progress bars)
        if matches!(self.current_window.widget_type.as_str(), "lefthand" | "righthand" | "spellhand" | "hands" | "progress") {
            Self::render_color_field(23, self.focused_field, "Text Color:", &mut self.text_color_input, content.x, y, content.width, buf, config);
            y += 3;
        }

        Self::render_dropdown(10, self.focused_field, "Content Align:", CONTENT_ALIGNS[self.content_align_index], self.content_align_index, CONTENT_ALIGNS.len(), content.x, y, content.width, buf);
        y += 2;

        // === WIDGET-SPECIFIC SETTINGS ===
        let has_widget_specific = matches!(self.current_window.widget_type.as_str(), "text" | "entity" | "lefthand" | "righthand" | "spellhand" | "countdown" | "progress")
            || self.current_window.widget_type == "active_effects";

        if has_widget_specific {
            Self::render_section_header("Widget-Specific", content.x, y, buf);
            y += 1;

            // Streams field only for text widgets and entity widgets
            if matches!(self.current_window.widget_type.as_str(), "text" | "entity") {
                Self::render_text_field(14, self.focused_field, "Streams:", &mut self.streams_input, content.x, y, content.width, buf);
                y += 3;
            }

            // Effect category dropdown only for active_effects widgets
            if self.current_window.widget_type == "active_effects" {
                Self::render_dropdown(19, self.focused_field, "Effect Category:",
                    EFFECT_CATEGORIES[self.effect_category_index],
                    self.effect_category_index,
                    EFFECT_CATEGORIES.len(),
                    content.x, y, content.width, buf);
                y += 2;
            }

            // Hand icon field only for hand widgets
            if matches!(self.current_window.widget_type.as_str(), "lefthand" | "righthand" | "spellhand") {
                Self::render_text_field(15, self.focused_field, "Hand Icon:", &mut self.hand_icon_input, content.x, y, content.width, buf);
                y += 3;
            }

            // Countdown-specific fields (countdown icon and bar color)
            if self.current_window.widget_type == "countdown" {
                Self::render_text_field(21, self.focused_field, "Countdown Icon:", &mut self.countdown_icon_input, content.x, y, content.width, buf);
                y += 3;
                Self::render_color_field(22, self.focused_field, "Bar Color:", &mut self.bar_color_input, content.x, y, content.width, buf, config);
                y += 3;
            }

            // Progress bar color (shared with countdown)
            if self.current_window.widget_type == "progress" {
                Self::render_color_field(22, self.focused_field, "Bar Color:", &mut self.bar_color_input, content.x, y, content.width, buf, config);
                y += 3;
            }

            // Compass colors (only for compass widgets)
            if self.current_window.widget_type == "compass" {
                Self::render_color_field(24, self.focused_field, "Active Color:", &mut self.compass_active_color_input, content.x, y, content.width, buf, config);
                y += 3;
                Self::render_color_field(25, self.focused_field, "Inactive Color:", &mut self.compass_inactive_color_input, content.x, y, content.width, buf, config);
                y += 3;
            }

            // Buffer size only for text windows (not tabbed - tabs have their own buffers)
            if self.current_window.widget_type == "text" {
                Self::render_text_field(16, self.focused_field, "Buffer Size:", &mut self.buffer_size_input, content.x, y, content.width, buf);
                y += 3;
            }
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

    fn render_color_field(field_id: usize, focused_field: usize, label: &str, textarea: &mut TextArea, x: u16, y: u16, width: u16, buf: &mut Buffer, config: &crate::config::Config) {
        let label_para = Paragraph::new(label);
        label_para.render(Rect { x, y, width: 15, height: 1 }, buf);

        let input_area = Rect {
            x: x + 15,
            y,
            width: width.saturating_sub(20),  // Leave space for preview
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

        // Draw color preview
        let color_text = textarea.lines()[0].to_string();
        if !color_text.is_empty() {
            // Resolve color name to hex
            let resolved_color = config.resolve_color(&color_text);
            if let Some(hex_color) = resolved_color {
                if let Some(color) = Self::parse_hex_color(&hex_color) {
                    // Draw preview block (███) to the right of input
                    let preview_x = x + width.saturating_sub(4);
                    for i in 0..3 {
                        if let Some(cell) = buf.cell_mut((preview_x + i, y + 1)) {
                            cell.set_char('█');
                            cell.set_fg(color);
                            cell.set_bg(Color::Black);
                        }
                    }
                }
            }
        }
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        if !hex.starts_with('#') || hex.len() != 7 {
            return None;
        }

        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;

        Some(Color::Rgb(r, g, b))
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

    fn render_section_header(text: &str, x: u16, y: u16, buf: &mut Buffer) {
        let header = Paragraph::new(format!("━━ {} ━━", text))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        header.render(Rect { x, y, width: 50, height: 1 }, buf);
    }

    fn render_buttons(focused_field: usize, x: u16, y: u16, buf: &mut Buffer) {
        let save_style = if focused_field == 17 {
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let save_btn = Paragraph::new("[ Save ]").style(save_style);
        save_btn.render(Rect { x, y, width: 10, height: 1 }, buf);

        let cancel_style = if focused_field == 18 {
            Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        let cancel_btn = Paragraph::new("[ Cancel ]").style(cancel_style);
        cancel_btn.render(Rect { x: x + 12, y, width: 12, height: 1 }, buf);
    }

    /// Get dynamic tab order that skips hidden fields based on widget type
    fn get_tab_order(&self) -> Vec<usize> {
        let widget_type = self.current_window.widget_type.as_str();

        // Base order for command_input (shorter, skips name and lock)
        if widget_type == "command_input" {
            return vec![9, 20, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 10, 17, 18];
        }

        // Build dynamic order for normal windows
        let mut order = vec![
            0,  // name
            9,  // title
            20, // show_title
            1,  // row
            2,  // col
            3,  // height
            4,  // width
            5,  // show_border
            6,  // border_style
            7,  // border_sides
            8,  // border_color
            11, // transparent_bg
            13, // lock
            12, // bg_color
        ];

        // Text Color (field 23) - only for hands and progress widgets
        if matches!(widget_type, "lefthand" | "righthand" | "spellhand" | "hands" | "progress") {
            order.push(23);
        }

        // Content Align (field 10)
        order.push(10);

        // Bar Color (field 22) - only for countdown and progress widgets
        if matches!(widget_type, "countdown" | "progress") {
            order.push(22);
        }

        // Compass Colors (fields 24, 25) - only for compass widgets
        if widget_type == "compass" {
            order.push(24); // active color
            order.push(25); // inactive color
        }

        // Streams (field 14) - only for text and entity widgets
        if matches!(widget_type, "text" | "entity") {
            order.push(14);
        }

        // Effect Category (field 19) - only for active_effects widgets
        if widget_type == "active_effects" {
            order.push(19);
        }

        // Hand Icon (field 15) - only for hand widgets
        if matches!(widget_type, "lefthand" | "righthand" | "spellhand") {
            order.push(15);
        }

        // Countdown Icon (field 21) - only for countdown widgets
        if widget_type == "countdown" {
            order.push(21);
        }

        // Buffer Size (field 16) - only for text windows (not tabbed)
        if widget_type == "text" {
            order.push(16);
        }

        // Save and Cancel buttons
        order.push(17);
        order.push(18);

        order
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
        self.is_dragging = false;
        // Reset position for next time
        self.popup_x = 0;
        self.popup_y = 0;
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool) -> bool {
        if !self.active {
            return false;
        }

        let popup_width = 100;
        let popup_height = 40;

        // Check if mouse is on title bar (top border, excluding corners)
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > self.popup_x
            && mouse_col < self.popup_x + popup_width - 1;

        if mouse_down && on_title_bar && !self.is_dragging {
            // Start dragging
            self.is_dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
            return true;
        }

        if self.is_dragging {
            if mouse_down {
                // Continue dragging - update position
                self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
                self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
                return true;
            } else {
                // Release - stop dragging
                self.is_dragging = false;
                return true;
            }
        }

        false
    }
}
