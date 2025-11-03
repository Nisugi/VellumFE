use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};
use tui_textarea::TextArea;
// use std::collections::HashMap; // unused

use crate::config::{DashboardIndicatorDef, TabConfig, WindowDef};

/// Result of window editor interaction
#[derive(Debug, Clone)]
pub enum WindowEditorResult {
    Save { window: WindowDef, is_new: bool, original_name: Option<String> },
    Cancel,
}

/// Editor mode determines which state the editor is in
#[derive(Debug, Clone, PartialEq)]
pub enum EditorMode {
    SelectingWindow,
    SelectingWidgetType,
    SelectingTemplate,
    EditingFields,
    EditingTabs,
    EditingIndicators,
}

/// Tab editor state
#[derive(Debug, Clone)]
struct TabEditorState {
    selected_index: usize,
    mode: TabEditMode,
    tab_name_input: TextArea<'static>,
    tab_stream_input: TextArea<'static>,
    editing_index: Option<usize>,
    focused_input: usize, // 0 = name, 1 = stream, 2 = show_timestamps checkbox
    show_timestamps: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum TabEditMode {
    Browsing,
    Adding,
    Editing,
}

/// Indicator editor state
#[derive(Debug, Clone)]
struct IndicatorEditorState {
    selected_index: usize,
    mode: IndicatorEditMode,
    picker_selected_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum IndicatorEditMode {
    Browsing,
    Picking,
}

// Available indicators for dashboard
const AVAILABLE_INDICATORS: &[(&str, &str)] = &[
    ("bleeding", "❤"),
    ("dead", "☠"),
    ("diseased", "☣"),
    ("invisible", "👁"),
    ("poisoned", "☠"),
    ("sitting", "↓"),
    ("standing", "↑"),
    ("stunned", "★"),
    ("webbed", "🕸"),
];

// Dropdown options
const BORDER_STYLES: &[&str] = &["none", "single", "double", "rounded", "thick"];
const CONTENT_ALIGNS: &[&str] = &["top-left", "top-center", "top-right", "center-left", "center", "center-right", "bottom-left", "bottom-center", "bottom-right"];
const TAB_BAR_POSITIONS: &[&str] = &["top", "bottom"];
const EFFECT_CATEGORIES: &[&str] = &["ActiveSpells", "Buffs", "Debuffs", "Cooldowns"];
const DASHBOARD_LAYOUTS: &[&str] = &["horizontal", "vertical", "grid_2x2", "grid_3x3"];

/// Get available templates for a widget type
fn get_templates_for_widget_type(widget_type: &str) -> Vec<&'static str> {
    match widget_type {
        "text" => vec!["thoughts", "speech", "familiar", "logons", "deaths", "arrivals", "ambients", "announcements", "loot", "bounty", "custom"],
        "tabbed" => vec!["chat", "custom"],
        "progress" => vec!["health", "mana", "stamina", "spirit", "bloodpoints", "stance", "encumbrance", "mindstate", "custom"],
        "countdown" => vec!["roundtime", "casttime", "stuntime", "custom"],
        "active_effects" => vec!["active_spells", "buffs", "debuffs", "cooldowns", "all_effects", "custom"],
        "entity" => vec!["targets", "players", "custom"],
        "dashboard" => vec!["status_dashboard", "custom"],
        "indicator" => vec!["bleeding", "dead", "diseased", "invisible", "poisoned", "sitting", "standing", "stunned", "webbed", "custom"],
        "compass" => vec!["compass"],
        "injury_doll" => vec!["injuries"],
        "hands" => vec!["lefthand", "righthand", "spellhand"],
        "inventory" => vec!["inventory"],
        "room" => vec!["room"],
        "map" => vec!["local_map"],
        "command_input" => vec!["command_input"],
        _ => vec!["custom"],
    }
}

/// Window editor widget - version 3 with fixed 70x20 layout
pub struct WindowEditor {
    // Mode
    pub mode: EditorMode,
    pub active: bool,

    // Popup position (fixed at col 5, row 1, moveable by drag)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,

    // Window selection (for edit mode)
    available_windows: Vec<String>,
    selected_window_index: usize,

    // Existing window names (for conflict detection when creating new windows)
    existing_window_names: Vec<String>,

    // Widget type selection (for new window mode)
    available_widget_types: Vec<String>,
    selected_widget_type_index: usize,

    // Template selection (after widget type is chosen)
    available_templates: Vec<String>,
    selected_template_index: usize,

    // Editing state
    is_new_window: bool,
    original_window_name: Option<String>,
    current_window: WindowDef,

    // Form fields with focused field tracking
    focused_field: usize,

    // Text input fields using TextArea (maroon background)
    name_input: TextArea<'static>,
    title_input: TextArea<'static>,
    row_input: TextArea<'static>,
    col_input: TextArea<'static>,
    rows_input: TextArea<'static>,
    cols_input: TextArea<'static>,
    min_rows_input: TextArea<'static>,
    min_cols_input: TextArea<'static>,
    max_rows_input: TextArea<'static>,
    max_cols_input: TextArea<'static>,
    buffer_size_input: TextArea<'static>,
    streams_input: TextArea<'static>,
    border_color_input: TextArea<'static>,
    bg_color_input: TextArea<'static>,
    text_color_input: TextArea<'static>,
    bar_color_input: TextArea<'static>,
    bar_bg_color_input: TextArea<'static>,
    compass_active_color_input: TextArea<'static>,
    compass_inactive_color_input: TextArea<'static>,
    progress_id_input: TextArea<'static>,
    countdown_id_input: TextArea<'static>,
    countdown_icon_input: TextArea<'static>,
    hand_icon_input: TextArea<'static>,
    tab_unread_prefix_input: TextArea<'static>,
    effect_default_color_input: TextArea<'static>,
    dashboard_spacing_input: TextArea<'static>,

    // Dropdown states
    content_align_index: usize,
    border_style_index: usize,
    tab_bar_position_index: usize,
    effect_category_index: usize,
    dashboard_layout_index: usize,

    // Checkbox states
    show_title: bool,
    lock_window: bool,
    transparent_bg: bool,
    show_border: bool,
    border_top: bool,
    border_bottom: bool,
    border_left: bool,
    border_right: bool,
    dashboard_hide_inactive: bool,
    numbers_only: bool,  // For progress bars: show only numbers, no text
    show_timestamps: bool,  // For text windows: show timestamps at end of lines

    // Tab editor state
    tab_editor: TabEditorState,

    // Indicator editor state
    indicator_editor: IndicatorEditorState,

    // Status message
    status_message: String,
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
            existing_window_names: Vec::new(),
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
                "inventory".to_string(),
                "room".to_string(),
                "map".to_string(),
                "command_input".to_string(),
                "spacer".to_string(),
            ],
            selected_widget_type_index: 0,
            available_templates: Vec::new(),
            selected_template_index: 0,
            is_new_window: false,
            original_window_name: None,
            current_window: WindowDef::default(),
            focused_field: 0,
            name_input: Self::create_textarea(24),
            title_input: Self::create_textarea(24),
            row_input: Self::create_textarea(8),
            col_input: Self::create_textarea(8),
            rows_input: Self::create_textarea(8),
            cols_input: Self::create_textarea(8),
            min_rows_input: Self::create_textarea(8),
            min_cols_input: Self::create_textarea(8),
            max_rows_input: Self::create_textarea(8),
            max_cols_input: Self::create_textarea(8),
            buffer_size_input: Self::create_textarea(10),
            streams_input: Self::create_textarea(50),
            border_color_input: Self::create_textarea(10),
            bg_color_input: Self::create_textarea(10),
            text_color_input: Self::create_textarea(10),
            bar_color_input: Self::create_textarea(10),
            bar_bg_color_input: Self::create_textarea(10),
            compass_active_color_input: Self::create_textarea(10),
            compass_inactive_color_input: Self::create_textarea(10),
            progress_id_input: Self::create_textarea(18),
            countdown_id_input: Self::create_textarea(10),
            countdown_icon_input: Self::create_textarea(10),
            hand_icon_input: Self::create_textarea(10),
            tab_unread_prefix_input: Self::create_textarea(10),
            effect_default_color_input: Self::create_textarea(10),
            dashboard_spacing_input: Self::create_textarea(8),
            content_align_index: 0,
            border_style_index: 1,
            tab_bar_position_index: 0,
            effect_category_index: 0,
            dashboard_layout_index: 0,
            show_title: true,
            lock_window: false,
            transparent_bg: false,
            show_border: true,
            border_top: true,
            border_bottom: true,
            border_left: true,
            border_right: true,
            dashboard_hide_inactive: false,
            numbers_only: false,
            show_timestamps: false,
            tab_editor: TabEditorState {
                selected_index: 0,
                mode: TabEditMode::Browsing,
                tab_name_input: Self::create_textarea(20),
                tab_stream_input: Self::create_textarea(20),
                editing_index: None,
                focused_input: 0,
                show_timestamps: false,
            },
            indicator_editor: IndicatorEditorState {
                selected_index: 0,
                mode: IndicatorEditMode::Browsing,
                picker_selected_index: 0,
            },
            status_message: String::new(),
        }
    }

    fn create_textarea(_max_width: usize) -> TextArea<'static> {
        let mut ta = TextArea::default();
        ta.set_cursor_line_style(Style::default());
        ta.set_max_histories(0);
        ta
    }

    /// Open editor for creating a new window
    pub fn open_for_new_window(&mut self, existing_window_names: Vec<String>) {
        self.mode = EditorMode::SelectingWidgetType;
        self.selected_widget_type_index = 0;
        self.existing_window_names = existing_window_names;
        self.active = true;
        self.is_new_window = true;
        self.status_message = "↑/↓: Navigate | Enter: Select widget type | Esc: Cancel".to_string();
    }

    pub fn open_for_new_window_with_type(&mut self, widget_type: String) {
        self.is_new_window = true;
        self.original_window_name = None;
        self.current_window = WindowDef::default();
        self.current_window.widget_type = widget_type.clone();
        self.current_window.name = "new_window".to_string();
        self.apply_widget_defaults(&widget_type);
        self.populate_fields_from_window();
        self.mode = EditorMode::EditingFields;
        self.active = true;

        // Set initial focused field
        self.focused_field = if widget_type == "command_input" { 1 } else { 0 };
        self.update_status();
    }

    /// Open editor for editing an existing window
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

    pub fn load_window_for_editing(&mut self, window: WindowDef) {
        self.is_new_window = false;
        self.original_window_name = Some(window.name.clone());
        self.current_window = window.clone();
        self.populate_fields_from_window();
        self.mode = EditorMode::EditingFields;
        self.active = true;

        // Set initial focused field
        self.focused_field = if window.widget_type == "command_input" { 1 } else { 0 };
        self.update_status();
    }

    /// Alias for load_window_for_editing (for compatibility)
    pub fn load_window(&mut self, window: WindowDef) {
        self.load_window_for_editing(window);
    }

    /// Get the name of the currently selected window (when in SelectingWindow mode)
    pub fn get_selected_window_name(&self) -> Option<String> {
        if self.mode == EditorMode::SelectingWindow && !self.available_windows.is_empty() {
            Some(
                self.available_windows
                    .get(self.selected_window_index.min(self.available_windows.len().saturating_sub(1)))
                    .cloned()
                    .unwrap_or_default()
            )
        } else {
            None
        }
    }

    fn apply_widget_defaults(&mut self, widget_type: &str) {
        match widget_type {
            "compass" => {
                self.current_window.rows = 5;
                self.current_window.cols = 10;
                self.current_window.compass_active_color = Some("#00ff00".to_string());
                self.current_window.compass_inactive_color = Some("#333333".to_string());
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
                self.current_window.buffer_size = 5000;
            },
            "tabbed" => {
                self.current_window.rows = 20;
                self.current_window.cols = 60;
                self.current_window.buffer_size = 5000;
            },
            "entity" => {
                self.current_window.rows = 10;
                self.current_window.cols = 30;
            },
            "active_effects" => {
                self.current_window.rows = 15;
                self.current_window.cols = 35;
            },
            "hands" => {
                self.current_window.rows = 3;
                self.current_window.cols = 20;
            },
            "inventory" => {
                self.current_window.rows = 20;
                self.current_window.cols = 60;
                self.current_window.streams = vec!["inv".to_string()];
            },
            "room" => {
                self.current_window.rows = 10;
                self.current_window.cols = 60;
                self.current_window.streams = vec!["room".to_string()];
            },
            "map" => {
                self.current_window.rows = 25;
                self.current_window.cols = 40;
            },
            _ => {
                self.current_window.rows = 10;
                self.current_window.cols = 40;
            }
        }
    }

    fn populate_fields_from_window(&mut self) {
        // Name
        self.name_input.delete_line_by_head();
        self.name_input.insert_str(&self.current_window.name);

        // Title
        self.title_input.delete_line_by_head();
        if let Some(ref title) = self.current_window.title {
            if !title.is_empty() {
                self.title_input.insert_str(title);
            }
        }

        // Position and size
        self.row_input.delete_line_by_head();
        self.row_input.insert_str(&self.current_window.row.to_string());

        self.col_input.delete_line_by_head();
        self.col_input.insert_str(&self.current_window.col.to_string());

        self.rows_input.delete_line_by_head();
        self.rows_input.insert_str(&self.current_window.rows.to_string());

        self.cols_input.delete_line_by_head();
        self.cols_input.insert_str(&self.current_window.cols.to_string());

        // Min/Max (optional fields)
        self.min_rows_input.delete_line_by_head();
        if let Some(min_rows) = self.current_window.min_rows {
            self.min_rows_input.insert_str(&min_rows.to_string());
        }

        self.min_cols_input.delete_line_by_head();
        if let Some(min_cols) = self.current_window.min_cols {
            self.min_cols_input.insert_str(&min_cols.to_string());
        }

        self.max_rows_input.delete_line_by_head();
        if let Some(max_rows) = self.current_window.max_rows {
            self.max_rows_input.insert_str(&max_rows.to_string());
        }

        self.max_cols_input.delete_line_by_head();
        if let Some(max_cols) = self.current_window.max_cols {
            self.max_cols_input.insert_str(&max_cols.to_string());
        }

        // Content align
        self.content_align_index = CONTENT_ALIGNS.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.content_align)
            .unwrap_or(0);

        // Border style
        self.border_style_index = BORDER_STYLES.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.border_style)
            .unwrap_or(1);

        // Checkboxes
        self.show_title = !matches!(&self.current_window.title, Some(t) if t.is_empty());
        self.lock_window = self.current_window.locked;
        self.transparent_bg = self.current_window.transparent_background;
        self.show_border = self.current_window.show_border;
        self.numbers_only = self.current_window.numbers_only;
        self.show_timestamps = self.current_window.show_timestamps.unwrap_or(false);

        // Border sides
        if let Some(ref sides) = self.current_window.border_sides {
            self.border_top = sides.contains(&"top".to_string());
            self.border_bottom = sides.contains(&"bottom".to_string());
            self.border_left = sides.contains(&"left".to_string());
            self.border_right = sides.contains(&"right".to_string());
        } else {
            self.border_top = true;
            self.border_bottom = true;
            self.border_left = true;
            self.border_right = true;
        }

        // Colors (with defaults)
        self.border_color_input.delete_line_by_head();
        let border_color = self.current_window.border_color.as_deref().unwrap_or("#808080");
        self.border_color_input.insert_str(border_color);

        self.bg_color_input.delete_line_by_head();
        // Handle three-state: None = inherit, Some("-") = transparent, Some(value) = use value
        if let Some(ref bg) = self.current_window.background_color {
            self.bg_color_input.insert_str(bg);
        }
        // If None, leave empty (will inherit from global config)

        // Buffer size
        self.buffer_size_input.delete_line_by_head();
        self.buffer_size_input.insert_str(&self.current_window.buffer_size.to_string());

        // Streams
        self.streams_input.delete_line_by_head();
        self.streams_input.insert_str(&self.current_window.streams.join(", "));

        // Widget-specific fields (with defaults)
        // Note: Some input fields are reused for different purposes depending on widget type

        if self.current_window.widget_type == "injury_doll" {
            // For injury_doll, text_color_input is injury1 (field 48)
            self.text_color_input.delete_line_by_head();
            let injury1 = self.current_window.injury1_color.as_deref().unwrap_or("#aa5500");
            self.text_color_input.insert_str(injury1);

            // bar_color_input is scar1 (field 51)
            self.bar_color_input.delete_line_by_head();
            let scar1 = self.current_window.scar1_color.as_deref().unwrap_or("#999999");
            self.bar_color_input.insert_str(scar1);

            // bar_bg_color_input is injury2 (field 49)
            self.bar_bg_color_input.delete_line_by_head();
            let injury2 = self.current_window.injury2_color.as_deref().unwrap_or("#ff8800");
            self.bar_bg_color_input.insert_str(injury2);
        } else {
            // For other widgets, use normal color fields
            self.text_color_input.delete_line_by_head();
            let text_color = self.current_window.text_color.as_deref().unwrap_or("#ffffff");
            self.text_color_input.insert_str(text_color);

            self.bar_color_input.delete_line_by_head();
            let bar_color = self.current_window.bar_fill.as_deref().unwrap_or("#00ff00");
            self.bar_color_input.insert_str(bar_color);

            self.bar_bg_color_input.delete_line_by_head();
            let bar_bg_color = self.current_window.bar_background.as_deref().unwrap_or("#333333");
            self.bar_bg_color_input.insert_str(bar_bg_color);
        }

        self.progress_id_input.delete_line_by_head();
        if self.current_window.widget_type == "injury_doll" {
            // For injury_doll, progress_id_input is used for default injury color (field 54)
            let injury_default = self.current_window.injury_default_color.as_deref().unwrap_or("#333333");
            self.progress_id_input.insert_str(injury_default);
        } else if let Some(ref id) = self.current_window.progress_id {
            self.progress_id_input.insert_str(id);
        }

        self.countdown_id_input.delete_line_by_head();
        if let Some(ref id) = self.current_window.countdown_id {
            self.countdown_id_input.insert_str(id);
        }

        self.countdown_icon_input.delete_line_by_head();
        let countdown_icon = self.current_window.countdown_icon.as_deref().unwrap_or("");
        self.countdown_icon_input.insert_str(countdown_icon);

        if self.current_window.widget_type == "injury_doll" {
            // For injury_doll, compass_active_color_input is scar2 (field 52)
            self.compass_active_color_input.delete_line_by_head();
            let scar2 = self.current_window.scar2_color.as_deref().unwrap_or("#777777");
            self.compass_active_color_input.insert_str(scar2);

            // compass_inactive_color_input is injury3 (field 50)
            self.compass_inactive_color_input.delete_line_by_head();
            let injury3 = self.current_window.injury3_color.as_deref().unwrap_or("#ff0000");
            self.compass_inactive_color_input.insert_str(injury3);
        } else {
            // For other widgets, use compass colors
            self.compass_active_color_input.delete_line_by_head();
            let compass_active = self.current_window.compass_active_color.as_deref().unwrap_or("#00ff00");
            self.compass_active_color_input.insert_str(compass_active);

            self.compass_inactive_color_input.delete_line_by_head();
            let compass_inactive = self.current_window.compass_inactive_color.as_deref().unwrap_or("#333333");
            self.compass_inactive_color_input.insert_str(compass_inactive);
        }

        self.hand_icon_input.delete_line_by_head();
        if self.current_window.widget_type == "indicator" {
            // For indicator, hand_icon_input is used for indicator icon (field 47)
            // Default to a simple circle icon
            self.hand_icon_input.insert_str("●");
        } else if let Some(ref icon) = self.current_window.hand_icon {
            // For hands widgets
            self.hand_icon_input.insert_str(icon);
        } else if matches!(self.current_window.widget_type.as_str(), "hands" | "lefthand" | "righthand" | "spellhand") {
            // Default hand icon
            self.hand_icon_input.insert_str("✋");
        }

        // Tabbed window fields
        self.tab_bar_position_index = TAB_BAR_POSITIONS.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.tab_bar_position)
            .unwrap_or(0);

        self.tab_unread_prefix_input.delete_line_by_head();
        let tab_prefix = self.current_window.tab_unread_prefix.as_deref().unwrap_or("* ");
        self.tab_unread_prefix_input.insert_str(tab_prefix);

        // Active effects fields
        self.effect_category_index = EFFECT_CATEGORIES.iter()
            .position(|&s| Some(s.to_string()) == self.current_window.effect_category)
            .unwrap_or(4);

        if self.current_window.widget_type == "injury_doll" {
            // For injury_doll, effect_default_color_input is scar3 (field 53)
            self.effect_default_color_input.delete_line_by_head();
            let scar3 = self.current_window.scar3_color.as_deref().unwrap_or("#555555");
            self.effect_default_color_input.insert_str(scar3);
        } else {
            // For other widgets, use effect default color
            self.effect_default_color_input.delete_line_by_head();
            let effect_default = self.current_window.effect_default_color.as_deref().unwrap_or("#ffffff");
            self.effect_default_color_input.insert_str(effect_default);
        }

        // Dashboard fields
        self.dashboard_layout_index = if let Some(ref layout) = self.current_window.dashboard_layout {
            DASHBOARD_LAYOUTS.iter().position(|&s| s == layout).unwrap_or(0)
        } else {
            0
        };

        self.dashboard_spacing_input.delete_line_by_head();
        if let Some(spacing) = self.current_window.dashboard_spacing {
            self.dashboard_spacing_input.insert_str(&spacing.to_string());
        }

        self.dashboard_hide_inactive = self.current_window.dashboard_hide_inactive.unwrap_or(false);
    }

    fn save_fields_to_window(&mut self) {
        // Name
        if self.current_window.widget_type != "command_input" {
            self.current_window.name = self.name_input.lines()[0].to_string();
        }

        // Title
        let title = self.title_input.lines()[0].to_string();
        self.current_window.title = if !self.show_title {
            Some("".to_string())
        } else if title.is_empty() {
            None
        } else {
            Some(title)
        };

        // Position and size
        self.current_window.row = self.row_input.lines()[0].parse().unwrap_or(0);
        self.current_window.col = self.col_input.lines()[0].parse().unwrap_or(0);
        self.current_window.rows = self.rows_input.lines()[0].parse::<u16>().unwrap_or(10).max(1);
        self.current_window.cols = self.cols_input.lines()[0].parse::<u16>().unwrap_or(40).max(1);

        // Min/Max
        let min_rows_text = self.min_rows_input.lines()[0].to_string();
        self.current_window.min_rows = if min_rows_text.is_empty() {
            None
        } else {
            Some(min_rows_text.parse().unwrap_or(1))
        };

        let min_cols_text = self.min_cols_input.lines()[0].to_string();
        self.current_window.min_cols = if min_cols_text.is_empty() {
            None
        } else {
            Some(min_cols_text.parse().unwrap_or(1))
        };

        let max_rows_text = self.max_rows_input.lines()[0].to_string();
        self.current_window.max_rows = if max_rows_text.is_empty() {
            None
        } else {
            Some(max_rows_text.parse().unwrap_or(100))
        };

        let max_cols_text = self.max_cols_input.lines()[0].to_string();
        self.current_window.max_cols = if max_cols_text.is_empty() {
            None
        } else {
            Some(max_cols_text.parse().unwrap_or(100))
        };

        // Content align (clamp index)
        if !CONTENT_ALIGNS.is_empty() {
            let idx = self.content_align_index.min(CONTENT_ALIGNS.len() - 1);
            self.current_window.content_align = Some(CONTENT_ALIGNS[idx].to_string());
        } else {
            self.current_window.content_align = None;
        }

        // Border style (clamp index)
        if !BORDER_STYLES.is_empty() {
            let idx = self.border_style_index.min(BORDER_STYLES.len() - 1);
            self.current_window.border_style = Some(BORDER_STYLES[idx].to_string());
        } else {
            self.current_window.border_style = None;
        }

        // Checkboxes
        self.current_window.locked = self.lock_window;
        self.current_window.transparent_background = self.transparent_bg;
        self.current_window.show_border = self.show_border;
        self.current_window.numbers_only = self.numbers_only;
        self.current_window.show_timestamps = Some(self.show_timestamps);

        // Border sides
        let mut sides = Vec::new();
        if self.border_top { sides.push("top".to_string()); }
        if self.border_bottom { sides.push("bottom".to_string()); }
        if self.border_left { sides.push("left".to_string()); }
        if self.border_right { sides.push("right".to_string()); }
        self.current_window.border_sides = if sides.len() == 4 {
            None
        } else {
            Some(sides)
        };

        // Colors
        let border_color = self.border_color_input.lines()[0].to_string();
        self.current_window.border_color = if border_color.is_empty() { None } else { Some(border_color) };

        // Handle background color three-state: empty = None (inherit), "-" = transparent, value = use value
        let bg_color = self.bg_color_input.lines()[0].to_string().trim().to_string();
        self.current_window.background_color = if bg_color.is_empty() {
            None  // Inherit from global config
        } else {
            Some(bg_color)  // Can be "-" for transparent or "#RRGGBB" for explicit color
        };

        // Buffer size
        self.current_window.buffer_size = self.buffer_size_input.lines()[0].parse::<usize>().unwrap_or(1000).max(100);

        // Streams
        let streams_text = self.streams_input.lines()[0].to_string();
        self.current_window.streams = if streams_text.trim().is_empty() {
            Vec::new()
        } else {
            streams_text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };

        // Widget-specific fields
        // Note: Input fields are reused for different purposes based on widget type
        if self.current_window.widget_type == "injury_doll" {
            // For injury_doll, save to injury/scar color fields
            let injury1 = self.text_color_input.lines()[0].to_string();
            self.current_window.injury1_color = if injury1.is_empty() { None } else { Some(injury1) };

            let scar1 = self.bar_color_input.lines()[0].to_string();
            self.current_window.scar1_color = if scar1.is_empty() { None } else { Some(scar1) };

            let injury2 = self.bar_bg_color_input.lines()[0].to_string();
            self.current_window.injury2_color = if injury2.is_empty() { None } else { Some(injury2) };

            let injury3 = self.compass_inactive_color_input.lines()[0].to_string();
            self.current_window.injury3_color = if injury3.is_empty() { None } else { Some(injury3) };

            let scar2 = self.compass_active_color_input.lines()[0].to_string();
            self.current_window.scar2_color = if scar2.is_empty() { None } else { Some(scar2) };

            let scar3 = self.effect_default_color_input.lines()[0].to_string();
            self.current_window.scar3_color = if scar3.is_empty() { None } else { Some(scar3) };

            let injury_default = self.progress_id_input.lines()[0].to_string();
            self.current_window.injury_default_color = if injury_default.is_empty() { None } else { Some(injury_default) };

            // Clear the normal color fields for injury_doll
            self.current_window.text_color = None;
            self.current_window.bar_fill = None;
            self.current_window.bar_background = None;
            self.current_window.compass_active_color = None;
            self.current_window.compass_inactive_color = None;
            self.current_window.effect_default_color = None;
            self.current_window.progress_id = None;
        } else {
            // For other widgets, save to normal color fields
            let text_color = self.text_color_input.lines()[0].to_string();
            self.current_window.text_color = if text_color.is_empty() { None } else { Some(text_color) };

            let bar_color = self.bar_color_input.lines()[0].to_string();
            self.current_window.bar_fill = if bar_color.is_empty() { None } else { Some(bar_color) };

            let bar_bg_color = self.bar_bg_color_input.lines()[0].to_string();
            self.current_window.bar_background = if bar_bg_color.is_empty() { None } else { Some(bar_bg_color) };

            // Clear injury_doll fields for other widgets
            self.current_window.injury_default_color = None;
            self.current_window.injury1_color = None;
            self.current_window.injury2_color = None;
            self.current_window.injury3_color = None;
            self.current_window.scar1_color = None;
            self.current_window.scar2_color = None;
            self.current_window.scar3_color = None;
        }

        // Only save these fields if not injury_doll (injury_doll uses these inputs for different purposes)
        if self.current_window.widget_type != "injury_doll" {
            let progress_id = self.progress_id_input.lines()[0].to_string();
            self.current_window.progress_id = if progress_id.is_empty() { None } else { Some(progress_id) };

            let compass_active = self.compass_active_color_input.lines()[0].to_string();
            self.current_window.compass_active_color = if compass_active.is_empty() { None } else { Some(compass_active) };

            let compass_inactive = self.compass_inactive_color_input.lines()[0].to_string();
            self.current_window.compass_inactive_color = if compass_inactive.is_empty() { None } else { Some(compass_inactive) };

            let effect_color = self.effect_default_color_input.lines()[0].to_string();
            self.current_window.effect_default_color = if effect_color.is_empty() { None } else { Some(effect_color) };
        }

        let countdown_id = self.countdown_id_input.lines()[0].to_string();
        self.current_window.countdown_id = if countdown_id.is_empty() { None } else { Some(countdown_id) };

        let countdown_icon = self.countdown_icon_input.lines()[0].to_string();
        self.current_window.countdown_icon = if countdown_icon.is_empty() { None } else { Some(countdown_icon) };

        let hand_icon = self.hand_icon_input.lines()[0].to_string();
        self.current_window.hand_icon = if hand_icon.is_empty() { None } else { Some(hand_icon) };

        // Tabbed window fields (clamp index to avoid OOB if options changed)
        if !TAB_BAR_POSITIONS.is_empty() {
            let idx = self
                .tab_bar_position_index
                .min(TAB_BAR_POSITIONS.len() - 1);
            self.current_window.tab_bar_position =
                Some(TAB_BAR_POSITIONS[idx].to_string());
        } else {
            self.current_window.tab_bar_position = None;
        }

        let tab_prefix = self.tab_unread_prefix_input.lines()[0].to_string();
        self.current_window.tab_unread_prefix = if tab_prefix.is_empty() { None } else { Some(tab_prefix) };

        // Active effects fields (clamp index to avoid OOB)
        if !EFFECT_CATEGORIES.is_empty() {
            let idx = self
                .effect_category_index
                .min(EFFECT_CATEGORIES.len() - 1);
            self.current_window.effect_category =
                Some(EFFECT_CATEGORIES[idx].to_string());
        } else {
            self.current_window.effect_category = None;
        }

        // Dashboard fields (clamp index to avoid OOB)
        if !DASHBOARD_LAYOUTS.is_empty() {
            let idx = self
                .dashboard_layout_index
                .min(DASHBOARD_LAYOUTS.len() - 1);
            self.current_window.dashboard_layout =
                Some(DASHBOARD_LAYOUTS[idx].to_string());
        } else {
            self.current_window.dashboard_layout = None;
        }

        let spacing_text = self.dashboard_spacing_input.lines()[0].to_string();
        self.current_window.dashboard_spacing = if spacing_text.is_empty() {
            None
        } else {
            Some(spacing_text.parse().unwrap_or(1))
        };

        self.current_window.dashboard_hide_inactive = Some(self.dashboard_hide_inactive);
    }

    fn update_status(&mut self) {
        self.status_message = match self.mode {
            EditorMode::SelectingWindow => "↑/↓: Navigate | Enter: Edit window | Esc: Cancel".to_string(),
            EditorMode::SelectingWidgetType => "↑/↓: Navigate | Enter: Select widget type | Esc: Cancel".to_string(),
            EditorMode::SelectingTemplate => "↑/↓: Navigate | Enter: Select template | Esc: Cancel".to_string(),
            EditorMode::EditingFields => "[Ctrl+S: Save]    [Esc: Cancel]".to_string(),
            EditorMode::EditingTabs => "↑/↓: Navigate | A: Add | E: Edit | D: Delete | Shift+↑/↓: Reorder | Esc: Back".to_string(),
            EditorMode::EditingIndicators => "↑/↓: Navigate | A: Add | Shift+↑/↓: Reorder | Del: Remove | Esc: Back".to_string(),
        };
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match self.mode {
            EditorMode::SelectingWindow => self.handle_selecting_window_key(key),
            EditorMode::SelectingWidgetType => self.handle_selecting_widget_type_key(key),
            EditorMode::SelectingTemplate => self.handle_selecting_template_key(key),
            EditorMode::EditingFields => self.handle_fields_key(key),
            EditorMode::EditingTabs => self.handle_tabs_key(key),
            EditorMode::EditingIndicators => self.handle_indicators_key(key),
        }
    }

    fn handle_selecting_window_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
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
                // This will be handled in app.rs which will call load_window
                None
            },
            _ => None,
        }
    }

    fn handle_selecting_widget_type_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
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
                // Get selected widget type and show templates
                let widget_type = self
                    .available_widget_types
                    .get(self.selected_widget_type_index.min(self.available_widget_types.len().saturating_sub(1)))
                    .cloned()
                    .unwrap_or_else(|| "text".to_string());
                self.available_templates = get_templates_for_widget_type(&widget_type)
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect();
                self.selected_template_index = 0;
                self.mode = EditorMode::SelectingTemplate;
                self.update_status();
                None
            },
            _ => None,
        }
    }

    fn handle_selecting_template_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match key.code {
            KeyCode::Esc => {
                // Go back to widget type selection
                self.mode = EditorMode::SelectingWidgetType;
                self.update_status();
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
                // Load the selected template
                let template_name = self
                    .available_templates
                    .get(self.selected_template_index.min(self.available_templates.len().saturating_sub(1)))
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());
                let widget_type = self
                    .available_widget_types
                    .get(self.selected_widget_type_index.min(self.available_widget_types.len().saturating_sub(1)))
                    .cloned()
                    .unwrap_or_else(|| "text".to_string());

                use crate::config::Config;
                if let Some(template) = Config::get_window_template(&template_name) {
                    self.current_window = template;

                    // Determine the window name
                    if template_name == "custom" {
                        self.current_window.name = "new_window".to_string();
                    } else {
                        // Check if template name conflicts with existing window
                        if self.existing_window_names.contains(&template_name) {
                            // Conflict: add _new suffix
                            self.current_window.name = format!("{}_new", template_name);
                        } else {
                            // No conflict: use template name as-is
                            self.current_window.name = template_name.clone();
                        }
                    }
                } else {
                    // Template not found - create a default window with the correct widget_type
                    self.current_window = WindowDef::default();
                    self.current_window.widget_type = widget_type.clone();
                    self.current_window.name = "new_window".to_string();
                    self.apply_widget_defaults(&widget_type);
                }
                self.is_new_window = true;
                self.original_window_name = None;
                self.populate_fields_from_window();
                self.mode = EditorMode::EditingFields;
                self.focused_field = 0;
                self.update_status();
                None
            },
            _ => None,
        }
    }

    fn handle_fields_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        // Field IDs for tab order
        // 0: name, 1: title, 2: row, 3: col, 4: rows, 5: cols
        // 6: min_rows, 7: min_cols, 8: max_rows, 9: max_cols
        // 10: content_align, 11: border_style
        // 12: show_title, 13: lock_window, 14: transparent_bg
        // 15: show_border, 16: border_top, 17: border_bottom, 18: border_left, 19: border_right
        // 20: border_color, 21: bg_color
        // Widget-specific fields start at 22+

        match key.code {
            KeyCode::Esc => {
                self.active = false;
                Some(WindowEditorResult::Cancel)
            },
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_fields_to_window();
                self.active = false;
                Some(WindowEditorResult::Save {
                    window: self.current_window.clone(),
                    is_new: self.is_new_window,
                    original_name: self.original_window_name.clone(),
                })
            },
            KeyCode::Tab => {
                // Forward
                let tab_order = self.get_tab_order();
                if let Some(pos) = tab_order.iter().position(|&f| f == self.focused_field) {
                    self.focused_field = if pos >= tab_order.len() - 1 {
                        tab_order[0]
                    } else {
                        tab_order[pos + 1]
                    };
                }
                None
            },
            KeyCode::BackTab => {
                let tab_order = self.get_tab_order();
                if let Some(pos) = tab_order.iter().position(|&f| f == self.focused_field) {
                    self.focused_field = if pos == 0 {
                        tab_order[tab_order.len() - 1]
                    } else {
                        tab_order[pos - 1]
                    };
                }
                None
            },
            KeyCode::Char(' ') if matches!(self.focused_field, 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 58 | 59 | 60) => {
                // Toggle checkboxes
                match self.focused_field {
                    12 => self.show_title = !self.show_title,
                    13 => self.lock_window = !self.lock_window,
                    14 => self.transparent_bg = !self.transparent_bg,
                    15 => self.show_border = !self.show_border,
                    16 => self.border_top = !self.border_top,
                    17 => self.border_bottom = !self.border_bottom,
                    18 => self.border_left = !self.border_left,
                    19 => self.border_right = !self.border_right,
                    58 => self.dashboard_hide_inactive = !self.dashboard_hide_inactive,
                    59 => self.numbers_only = !self.numbers_only,  // Progress bar Numbers Only
                    60 => self.show_timestamps = !self.show_timestamps,  // Text window Show Timestamps
                    _ => {}
                }
                None
            },
            KeyCode::Up if matches!(self.focused_field, 10 | 11 | 28 | 40 | 55) => {
                // Dropdowns
                match self.focused_field {
                    10 => self.content_align_index = self.content_align_index.saturating_sub(1),
                    11 => self.border_style_index = self.border_style_index.saturating_sub(1),
                    28 => self.tab_bar_position_index = self.tab_bar_position_index.saturating_sub(1),
                    40 => self.effect_category_index = self.effect_category_index.saturating_sub(1),
                    55 => self.dashboard_layout_index = self.dashboard_layout_index.saturating_sub(1),
                    _ => {}
                }
                None
            },
            KeyCode::Down if matches!(self.focused_field, 10 | 11 | 28 | 40 | 55) => {
                // Dropdowns
                match self.focused_field {
                    10 => self.content_align_index = (self.content_align_index + 1).min(CONTENT_ALIGNS.len() - 1),
                    11 => self.border_style_index = (self.border_style_index + 1).min(BORDER_STYLES.len() - 1),
                    28 => self.tab_bar_position_index = (self.tab_bar_position_index + 1).min(TAB_BAR_POSITIONS.len() - 1),
                    40 => self.effect_category_index = (self.effect_category_index + 1).min(EFFECT_CATEGORIES.len() - 1),
                    55 => self.dashboard_layout_index = (self.dashboard_layout_index + 1).min(DASHBOARD_LAYOUTS.len() - 1),
                    _ => {}
                }
                None
            },
            KeyCode::Enter if self.focused_field == 26 => {
                // Edit Tabs button
                self.mode = EditorMode::EditingTabs;
                self.update_status();
                None
            },
            KeyCode::Enter if self.focused_field == 56 => {
                // Edit Indicators button
                self.mode = EditorMode::EditingIndicators;
                self.update_status();
                None
            },
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+A to select all in current text field
                match self.focused_field {
                    0 => self.name_input.select_all(),
                    1 => self.title_input.select_all(),
                    2 => self.row_input.select_all(),
                    3 => self.col_input.select_all(),
                    4 => self.rows_input.select_all(),
                    5 => self.cols_input.select_all(),
                    6 => self.min_rows_input.select_all(),
                    7 => self.min_cols_input.select_all(),
                    8 => self.max_rows_input.select_all(),
                    9 => self.max_cols_input.select_all(),
                    20 => self.border_color_input.select_all(),
                    21 => self.bg_color_input.select_all(),
                    22 => self.buffer_size_input.select_all(),
                    23 => self.streams_input.select_all(),
                    24 => self.text_color_input.select_all(),
                    25 => self.bar_color_input.select_all(),
                    27 => self.bar_bg_color_input.select_all(),
                    29 => self.tab_unread_prefix_input.select_all(),
                    30 => self.text_color_input.select_all(),
                    31 => self.bar_color_input.select_all(),
                    32 => self.bar_bg_color_input.select_all(),
                    33 => self.progress_id_input.select_all(),
                    34 => self.bar_color_input.select_all(),
                    35 => self.bar_bg_color_input.select_all(),
                    36 => self.countdown_id_input.select_all(),
                    37 => self.countdown_icon_input.select_all(),
                    38 => self.effect_default_color_input.select_all(),
                    39 => self.text_color_input.select_all(),
                    41 => self.compass_active_color_input.select_all(),
                    42 => self.compass_inactive_color_input.select_all(),
                    43 => self.text_color_input.select_all(),
                    44 => self.hand_icon_input.select_all(),
                    45 => self.compass_active_color_input.select_all(),
                    46 => self.compass_inactive_color_input.select_all(),
                    47 => self.hand_icon_input.select_all(),
                    48 => self.text_color_input.select_all(),
                    49 => self.bar_bg_color_input.select_all(),
                    50 => self.compass_inactive_color_input.select_all(),
                    51 => self.bar_color_input.select_all(),
                    52 => self.compass_active_color_input.select_all(),
                    53 => self.effect_default_color_input.select_all(),
                    54 => self.progress_id_input.select_all(),
                    57 => self.dashboard_spacing_input.select_all(),
                    _ => {}
                }
                None
            },
            _ => {
                // Pass to text inputs
                use tui_textarea::Input;
                let input: Input = key.into();

                match self.focused_field {
                    0 => { self.name_input.input(input); },
                    1 => { self.title_input.input(input); },
                    2 => { self.row_input.input(input); },
                    3 => { self.col_input.input(input); },
                    4 => { self.rows_input.input(input); },
                    5 => { self.cols_input.input(input); },
                    6 => { self.min_rows_input.input(input); },
                    7 => { self.min_cols_input.input(input); },
                    8 => { self.max_rows_input.input(input); },
                    9 => { self.max_cols_input.input(input); },
                    20 => { self.border_color_input.input(input); },
                    21 => { self.bg_color_input.input(input); },
                    22 => { self.buffer_size_input.input(input); },
                    23 => { self.streams_input.input(input); },
                    24 => { self.text_color_input.input(input); }, // tab_active_color
                    25 => { self.bar_color_input.input(input); }, // tab_inactive_color
                    27 => { self.bar_bg_color_input.input(input); }, // tab_unread_color
                    29 => { self.tab_unread_prefix_input.input(input); },
                    30 => { self.text_color_input.input(input); }, // progress text_color
                    31 => { self.bar_color_input.input(input); }, // progress bar_color
                    32 => { self.bar_bg_color_input.input(input); }, // progress bar_bg_color
                    33 => { self.progress_id_input.input(input); },
                    34 => { self.bar_color_input.input(input); }, // countdown bar_color
                    35 => { self.bar_bg_color_input.input(input); }, // countdown bar_bg_color
                    36 => { self.countdown_id_input.input(input); },
                    37 => { self.countdown_icon_input.input(input); },
                    38 => { self.effect_default_color_input.input(input); },
                    39 => { self.text_color_input.input(input); }, // active_effects text_color
                    41 => { self.compass_active_color_input.input(input); },
                    42 => { self.compass_inactive_color_input.input(input); },
                    43 => { self.text_color_input.input(input); }, // hands text_color
                    44 => { self.hand_icon_input.input(input); },
                    45 => { self.compass_active_color_input.input(input); }, // indicator active
                    46 => { self.compass_inactive_color_input.input(input); }, // indicator inactive
                    47 => { self.hand_icon_input.input(input); }, // indicator icon
                    48 => { self.text_color_input.input(input); }, // injury1
                    49 => { self.bar_bg_color_input.input(input); }, // injury2
                    50 => { self.compass_inactive_color_input.input(input); }, // injury3
                    51 => { self.bar_color_input.input(input); }, // scar1
                    52 => { self.compass_active_color_input.input(input); }, // scar2
                    53 => { self.effect_default_color_input.input(input); }, // scar3
                    54 => { self.progress_id_input.input(input); }, // injury_default_color
                    57 => { self.dashboard_spacing_input.input(input); },
                    _ => {}
                }
                None
            }
        }
    }

    fn handle_tabs_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match self.tab_editor.mode {
            TabEditMode::Browsing => {
                match key.code {
                    KeyCode::Esc => {
                        self.mode = EditorMode::EditingFields;
                        self.update_status();
                        None
                    },
                    KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                        // Reorder tab up
                        if let Some(ref mut tabs) = self.current_window.tabs {
                            if self.tab_editor.selected_index > 0 && self.tab_editor.selected_index < tabs.len() {
                                tabs.swap(self.tab_editor.selected_index, self.tab_editor.selected_index - 1);
                                self.tab_editor.selected_index -= 1;
                            }
                        }
                        None
                    },
                    KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
                        // Reorder tab down
                        if let Some(ref mut tabs) = self.current_window.tabs {
                            if self.tab_editor.selected_index + 1 < tabs.len() {
                                tabs.swap(self.tab_editor.selected_index, self.tab_editor.selected_index + 1);
                                self.tab_editor.selected_index += 1;
                            }
                        }
                        None
                    },
                    KeyCode::Up => {
                        if self.tab_editor.selected_index > 0 {
                            self.tab_editor.selected_index -= 1;
                        }
                        None
                    },
                    KeyCode::Down => {
                        let tab_count = self.current_window.tabs.as_ref().map(|t| t.len()).unwrap_or(0);
                        if self.tab_editor.selected_index + 1 < tab_count {
                            self.tab_editor.selected_index += 1;
                        }
                        None
                    },
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        self.tab_editor.mode = TabEditMode::Adding;
                        self.tab_editor.tab_name_input.delete_line_by_head();
                        self.tab_editor.tab_stream_input.delete_line_by_head();
                        self.tab_editor.show_timestamps = false;
                        self.tab_editor.focused_input = 0;
                        None
                    },
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        if let Some(ref tabs) = self.current_window.tabs {
                            if !tabs.is_empty() {
                                let idx = self.tab_editor.selected_index.min(tabs.len() - 1);
                                let tab = &tabs[idx];
                                self.tab_editor.tab_name_input.delete_line_by_head();
                                self.tab_editor.tab_name_input.insert_str(&tab.name);
                                self.tab_editor.tab_stream_input.delete_line_by_head();
                                self.tab_editor.tab_stream_input.insert_str(&tab.stream);
                                self.tab_editor.show_timestamps = tab.show_timestamps.unwrap_or(false);
                                self.tab_editor.editing_index = Some(idx);
                                self.tab_editor.mode = TabEditMode::Editing;
                                self.tab_editor.focused_input = 0;
                            }
                        }
                        None
                    },
                    KeyCode::Char('d') | KeyCode::Char('D') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(ref mut tabs) = self.current_window.tabs {
                            if self.tab_editor.selected_index < tabs.len() {
                                tabs.remove(self.tab_editor.selected_index);
                                if self.tab_editor.selected_index >= tabs.len() && tabs.len() > 0 {
                                    self.tab_editor.selected_index = tabs.len() - 1;
                                }
                            }
                        }
                        None
                    },
                    _ => None,
                }
            },
            TabEditMode::Adding | TabEditMode::Editing => {
                match key.code {
                    KeyCode::Esc => {
                        self.tab_editor.mode = TabEditMode::Browsing;
                        None
                    },
                    KeyCode::Enter => {
                        let name = self.tab_editor.tab_name_input.lines()[0].to_string();
                        let stream = self.tab_editor.tab_stream_input.lines()[0].to_string();

                        if !name.is_empty() && !stream.is_empty() {
                            if self.current_window.tabs.is_none() {
                                self.current_window.tabs = Some(Vec::new());
                            }

                            if let Some(ref mut tabs) = self.current_window.tabs {
                                let show_timestamps_value = Some(self.tab_editor.show_timestamps);
                                if self.tab_editor.mode == TabEditMode::Editing {
                                    if let Some(idx) = self.tab_editor.editing_index {
                                        if idx < tabs.len() {
                                            tabs[idx] = TabConfig { name, stream, show_timestamps: show_timestamps_value };
                                        }
                                    }
                                } else {
                                    tabs.push(TabConfig { name, stream, show_timestamps: show_timestamps_value });
                                }
                            }

                            self.tab_editor.mode = TabEditMode::Browsing;
                        }
                        None
                    },
                    KeyCode::Tab => {
                        // Cycle through: name (0) -> stream (1) -> checkbox (2) -> name
                        self.tab_editor.focused_input = (self.tab_editor.focused_input + 1) % 3;
                        None
                    },
                    KeyCode::Char(' ') if self.tab_editor.focused_input == 2 => {
                        // Toggle show_timestamps checkbox
                        self.tab_editor.show_timestamps = !self.tab_editor.show_timestamps;
                        None
                    },
                    KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+A to select all in current text field
                        if self.tab_editor.focused_input == 0 {
                            self.tab_editor.tab_name_input.select_all();
                        } else if self.tab_editor.focused_input == 1 {
                            self.tab_editor.tab_stream_input.select_all();
                        }
                        None
                    },
                    _ => {
                        use tui_textarea::Input;
                        let input: Input = key.into();
                        // Route input to the focused field (only for text inputs)
                        if self.tab_editor.focused_input == 0 {
                            self.tab_editor.tab_name_input.input(input);
                        } else if self.tab_editor.focused_input == 1 {
                            self.tab_editor.tab_stream_input.input(input);
                        }
                        // Ignore input for checkbox (field 2)
                        None
                    }
                }
            }
        }
    }

    fn handle_indicators_key(&mut self, key: KeyEvent) -> Option<WindowEditorResult> {
        match self.indicator_editor.mode {
            IndicatorEditMode::Browsing => {
                match key.code {
                    KeyCode::Esc => {
                        self.mode = EditorMode::EditingFields;
                        self.update_status();
                        None
                    },
                    KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) || key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Reorder up
                        if let Some(ref mut indicators) = self.current_window.dashboard_indicators {
                            if self.indicator_editor.selected_index > 0 && self.indicator_editor.selected_index < indicators.len() {
                                indicators.swap(self.indicator_editor.selected_index, self.indicator_editor.selected_index - 1);
                                self.indicator_editor.selected_index -= 1;
                            }
                        }
                        None
                    },
                    KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) || key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Reorder down
                        if let Some(ref mut indicators) = self.current_window.dashboard_indicators {
                            if self.indicator_editor.selected_index + 1 < indicators.len() {
                                indicators.swap(self.indicator_editor.selected_index, self.indicator_editor.selected_index + 1);
                                self.indicator_editor.selected_index += 1;
                            }
                        }
                        None
                    },
                    KeyCode::Up => {
                        if self.indicator_editor.selected_index > 0 {
                            self.indicator_editor.selected_index -= 1;
                        }
                        None
                    },
                    KeyCode::Down => {
                        let count = self.current_window.dashboard_indicators.as_ref().map(|i| i.len()).unwrap_or(0);
                        if self.indicator_editor.selected_index + 1 < count {
                            self.indicator_editor.selected_index += 1;
                        }
                        None
                    },
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        self.indicator_editor.mode = IndicatorEditMode::Picking;
                        self.indicator_editor.picker_selected_index = 0;
                        None
                    },
                    KeyCode::Char('d') | KeyCode::Char('D') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(ref mut indicators) = self.current_window.dashboard_indicators {
                            if self.indicator_editor.selected_index < indicators.len() {
                                indicators.remove(self.indicator_editor.selected_index);
                                if self.indicator_editor.selected_index >= indicators.len() && indicators.len() > 0 {
                                    self.indicator_editor.selected_index = indicators.len() - 1;
                                }
                            }
                        }
                        None
                    },
                    _ => None,
                }
            },
            IndicatorEditMode::Picking => {
                match key.code {
                    KeyCode::Esc => {
                        self.indicator_editor.mode = IndicatorEditMode::Browsing;
                        None
                    },
                    KeyCode::Up => {
                        if self.indicator_editor.picker_selected_index > 0 {
                            self.indicator_editor.picker_selected_index -= 1;
                        }
                        None
                    },
                    KeyCode::Down => {
                        if self.indicator_editor.picker_selected_index + 1 < AVAILABLE_INDICATORS.len() {
                            self.indicator_editor.picker_selected_index += 1;
                        }
                        None
                    },
                    KeyCode::Enter => {
                        let (id, icon) = if AVAILABLE_INDICATORS.is_empty() {
                            ("", "")
                        } else {
                            AVAILABLE_INDICATORS[
                                self.indicator_editor
                                    .picker_selected_index
                                    .min(AVAILABLE_INDICATORS.len() - 1)
                            ]
                        };

                        if self.current_window.dashboard_indicators.is_none() {
                            self.current_window.dashboard_indicators = Some(Vec::new());
                        }

                        if let Some(ref mut indicators) = self.current_window.dashboard_indicators {
                            indicators.push(DashboardIndicatorDef {
                                id: id.to_string(),
                                icon: icon.to_string(),
                                colors: vec![],
                            });
                        }

                        self.indicator_editor.mode = IndicatorEditMode::Browsing;
                        None
                    },
                    _ => None,
                }
            }
        }
    }

    /// Get dynamic tab order based on widget type
    fn get_tab_order(&self) -> Vec<usize> {
        let widget_type = self.current_window.widget_type.as_str();

        let mut order = Vec::new();

        // Skip name for command_input
        if widget_type != "command_input" {
            order.push(0); // name
        }

        order.push(1); // title
        order.push(2); // row
        order.push(3); // col
        order.push(4); // rows
        order.push(5); // cols
        order.push(6); // min_rows
        order.push(7); // min_cols
        order.push(8); // max_rows
        order.push(9); // max_cols
        order.push(10); // content_align
        order.push(11); // border_style
        order.push(12); // show_title

        if widget_type != "command_input" {
            order.push(13); // lock_window
        }

        order.push(14); // transparent_bg
        order.push(21); // bg_color
        order.push(15); // show_border
        order.push(16); // border_top
        order.push(17); // border_bottom
        order.push(18); // border_left
        order.push(19); // border_right
        order.push(20); // border_color

        // Widget-specific fields
        match widget_type {
            "text" => {
                order.push(22); // buffer_size
                order.push(23); // streams
                order.push(60); // show_timestamps
            },
            "entity" => {
                order.push(22); // buffer_size
                order.push(23); // streams
            },
            "tabbed" => {
                order.push(26); // edit_tabs button
                order.push(28); // tab_bar_position
                order.push(29); // tab_unread_prefix (new_msg)
                order.push(24); // tab_active_color
                order.push(25); // tab_inactive_color
                order.push(27); // tab_unread_color
            },
            "progress" => {
                order.push(33); // progress_id
                order.push(30); // text_color
                order.push(31); // bar_color
                order.push(32); // bar_bg_color
                order.push(59); // numbers_only checkbox
            },
            "countdown" => {
                order.push(37); // countdown_icon
                order.push(36); // countdown_id
                order.push(34); // bar_color
                order.push(35); // bar_bg_color
            },
            "active_effects" => {
                order.push(40); // effect_category
                order.push(39); // text_color
                order.push(38); // effect_default_color
            },
            "compass" => {
                order.push(41); // compass_active_color
                order.push(42); // compass_inactive_color
            },
            "hands" | "lefthand" | "righthand" | "spellhand" => {
                order.push(44); // hand_icon
                order.push(43); // text_color
            },
            "indicator" => {
                order.push(47); // indicator_icon
                order.push(45); // indicator_active_color
                order.push(46); // indicator_inactive_color
            },
            "injury_doll" => {
                order.push(51); // scar1_color
                order.push(52); // scar2_color
                order.push(53); // scar3_color
                order.push(48); // injury1_color
                order.push(49); // injury2_color
                order.push(50); // injury3_color
                order.push(54); // injury_default_color
            },
            "dashboard" => {
                order.push(55); // dashboard_layout
                order.push(56); // edit_indicators button
                order.push(57); // dashboard_spacing
                order.push(58); // dashboard_hide_inactive
            },
            _ => {},
        }

        order
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        // Fixed 70x20 popup
        let popup_width = 70;
        let popup_height = 20;

        // Center on first render
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

        // Clear the popup area to prevent bleed-through
        Clear.render(popup_area, buf);

        // Fill background with black
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    let cell = &mut buf[(x, y)];
                    cell.set_char(' ').set_bg(Color::Black);
                }
            }
        }

        // Draw border and title
        let title = if self.is_new_window {
            " Add Window (drag title to move) "
        } else {
            " Edit Window (drag title to move) "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(Style::default().bg(Color::Black).fg(Color::Cyan));
        block.render(popup_area, buf);

        // Content area (inside borders, minus 1 for status line at bottom)
        let content = Rect {
            x: popup_area.x + 1,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(2),
            height: popup_area.height.saturating_sub(3), // -2 for borders, -1 for status line
        };

        match self.mode {
            EditorMode::SelectingWindow => self.render_window_selection(content, buf),
            EditorMode::SelectingWidgetType => self.render_widget_type_selection(content, buf),
            EditorMode::SelectingTemplate => self.render_template_selection(content, buf),
            EditorMode::EditingFields => self.render_fields(content, buf, config),
            EditorMode::EditingTabs => self.render_tab_editor(content, buf),
            EditorMode::EditingIndicators => self.render_indicator_editor(content, buf),
        }

        // Status line at bottom
        let status_y = popup_area.y + popup_area.height - 2;
        let status = Paragraph::new(&self.status_message as &str)
            .style(Style::default().fg(Color::Yellow));
        status.render(
            Rect {
                x: content.x,
                y: status_y,
                width: content.width,
                height: 1,
            },
            buf,
        );
    }

    fn render_window_selection(&self, area: Rect, buf: &mut Buffer) {
        // Calculate scroll offset to keep selected item visible
        let visible_height = area.height as usize;
        let total_items = self.available_windows.len();

        // Calculate start index for scrolling
        let scroll_offset = if self.selected_window_index >= visible_height {
            self.selected_window_index - visible_height + 1
        } else {
            0
        };

        // Render visible portion of the list
        let mut y = area.y;
        for i in scroll_offset..total_items.min(scroll_offset + visible_height) {
            let window_name = &self.available_windows[i];
            let prefix = if i == self.selected_window_index { "> " } else { "  " };
            let style = if i == self.selected_window_index {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            buf.set_string(area.x, y, format!("{}{}", prefix, window_name), style);
            y += 1;
        }
    }

    fn render_widget_type_selection(&self, area: Rect, buf: &mut Buffer) {
        // Render list of available widget types
        let mut y = area.y;
        for (i, widget_type) in self.available_widget_types.iter().enumerate() {
            let prefix = if i == self.selected_widget_type_index { "> " } else { "  " };
            let style = if i == self.selected_widget_type_index {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            buf.set_string(area.x, y, format!("{}{}", prefix, widget_type), style);
            y += 1;
            if y >= area.y + area.height {
                break;
            }
        }
    }

    fn render_template_selection(&self, area: Rect, buf: &mut Buffer) {
        // Render list of available templates
        let mut y = area.y;
        for (i, template_name) in self.available_templates.iter().enumerate() {
            let prefix = if i == self.selected_template_index { "> " } else { "  " };
            let style = if i == self.selected_template_index {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            buf.set_string(area.x, y, format!("{}{}", prefix, template_name), style);
            y += 1;
            if y >= area.y + area.height {
                break;
            }
        }
    }

    fn render_fields(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        let left_x = area.x;  // Left side starts at x
        let right_x = area.x + 36;  // Right side starts at x+36 (1 padding + 32 cols + 3 padding)
        let widget_type = self.current_window.widget_type.as_str();
        let mut y = area.y;

        // === COMMON FIELDS (all widget types) ===

        // Row 0: Name: + 2 spaces + 23 chars | Show Title checkbox
        if widget_type != "command_input" {
            Self::render_inline_textarea_with_spacing(self.focused_field, 0, "Name:", &mut self.name_input, left_x, y, 23, 2, buf, config);
            self.render_checkbox(12, "Show Title", self.show_title, right_x, y, buf);
            y += 1;
        } else {
            // command_input: skip Name, start with Title
            self.render_checkbox(12, "Show Title", self.show_title, right_x, y, buf);
            y += 1;
        }

        // Row 1: blank on left | Lock Window checkbox
        self.render_checkbox(13, "Lock Window", self.lock_window, right_x, y, buf);
        y += 1;

        // Row 2: Title: + 1 space + 23 chars | Transparent BG checkbox
        Self::render_inline_textarea_with_hint(self.focused_field, 1, "Title:", &mut self.title_input, left_x, y, 23, 1, Some("display name"), buf, config);
        self.render_checkbox(14, "Transparent BG", self.transparent_bg, right_x, y, buf);
        y += 1;

        // Row 3: blank | BG Color: + 5 spaces + 10 chars + 2 spaces + preview
        Self::render_inline_textarea_with_spacing(self.focused_field, 21, "BG Color:", &mut self.bg_color_input, right_x, y, 10, 5, buf, config);
        let preview_x = right_x + 9 + 5 + 10 + 2; // "BG Color:" (9) + 5 spaces + 10 chars + 2 spaces
        self.render_color_preview(&self.bg_color_input.lines()[0].to_string(), preview_x, y, buf, config);
        y += 1;

        // Row 4: Row: + 2 spaces + 8 chars, 2 spaces, Col: + 2 spaces + 8 chars
        Self::render_inline_textarea_with_spacing(self.focused_field, 2, "Row:", &mut self.row_input, left_x, y, 8, 2, buf, config);
        // Col starts at: "Row:" (4) + 2 spaces + 8 chars + 2 spaces = 16
        Self::render_inline_textarea_with_spacing(self.focused_field, 3, "Col:", &mut self.col_input, left_x + 16, y, 8, 2, buf, config);
        y += 1;

        // Row 5: Rows: + 1 space + 8 chars, 2 spaces, Cols: + 1 space + 8 chars | Show Border checkbox (not for spacer)
        Self::render_inline_textarea_with_hint(self.focused_field, 4, "Rows:", &mut self.rows_input, left_x, y, 8, 1, Some("height"), buf, config);
        // Cols starts at: "Rows:" (5) + 1 space + 8 chars + 2 spaces = 16
        Self::render_inline_textarea_with_hint(self.focused_field, 5, "Cols:", &mut self.cols_input, left_x + 16, y, 8, 1, Some("width"), buf, config);
        if widget_type != "spacer" { self.render_checkbox(15, "Show Border", self.show_border, right_x, y, buf); }
        y += 1;

        // Row 6: Min: + 2 spaces + 8 chars, 2 spaces, Min: + 2 spaces + 8 chars | Top Border checkbox (not for spacer)
        Self::render_inline_textarea_with_hint(self.focused_field, 6, "Min:", &mut self.min_rows_input, left_x, y, 8, 2, Some("height"), buf, config);
        // Second Min starts at: "Min:" (4) + 2 spaces + 8 chars + 2 spaces = 16
        Self::render_inline_textarea_with_hint(self.focused_field, 7, "Min:", &mut self.min_cols_input, left_x + 16, y, 8, 2, Some("width"), buf, config);
        if widget_type != "spacer" { self.render_checkbox(16, "Top Border", self.border_top, right_x, y, buf); }
        y += 1;

        // Row 7: Max: + 2 spaces + 8 chars, 2 spaces, Max: + 2 spaces + 8 chars | Bottom Border checkbox (not for spacer)
        Self::render_inline_textarea_with_hint(self.focused_field, 8, "Max:", &mut self.max_rows_input, left_x, y, 8, 2, Some("height"), buf, config);
        // Second Max starts at: "Max:" (4) + 2 spaces + 8 chars + 2 spaces = 16
        Self::render_inline_textarea_with_hint(self.focused_field, 9, "Max:", &mut self.max_cols_input, left_x + 16, y, 8, 2, Some("width"), buf, config);
        if widget_type != "spacer" { self.render_checkbox(17, "Bottom Border", self.border_bottom, right_x, y, buf); }
        y += 1;

        // Row 8: blank on left | Left Border checkbox (not for spacer)
        if widget_type != "spacer" { self.render_checkbox(18, "Left Border", self.border_left, right_x, y, buf); }
        y += 1;

        // Row 9: Content Align dropdown | Right Border checkbox (not for spacer)
        if widget_type != "spacer" {
            let ca = if CONTENT_ALIGNS.is_empty() { "" } else { CONTENT_ALIGNS[self.content_align_index.min(CONTENT_ALIGNS.len() - 1)] };
            self.render_dropdown(10, "Content Align:", ca, left_x, y, buf);
            self.render_checkbox(19, "Right Border", self.border_right, right_x, y, buf);
        }
        y += 1;

        // Row 10: Border Style dropdown (2 spaces) | Border Color (not for spacer)
        if widget_type != "spacer" {
            let bs = if BORDER_STYLES.is_empty() { "" } else { BORDER_STYLES[self.border_style_index.min(BORDER_STYLES.len() - 1)] };
            self.render_dropdown_with_spacing(11, "Border Style:", bs, left_x, y, 2, buf);
            Self::render_inline_textarea_with_spacing(self.focused_field, 20, "Border Color:", &mut self.border_color_input, right_x, y, 10, 1, buf, config);
            let border_preview_x = right_x + 13 + 1 + 10 + 2; // "Border Color:" (13) + 1 space + 10 chars + 2 spaces
            self.render_color_preview(&self.border_color_input.lines()[0].to_string(), border_preview_x, y, buf, config);
        }
        y += 1;

        // Row 11: blank
        y += 1;

        // === WIDGET-SPECIFIC SECTIONS starting at Row 12 ===

        match widget_type {
            "text" => {
                // Row 12: Buffer Size input (8 chars, left) | Streams input (21 chars, right)
                Self::render_inline_textarea(self.focused_field, 22, "Buffer Size:", &mut self.buffer_size_input, left_x, y, 8, buf, config);
                Self::render_inline_textarea(self.focused_field, 23, "Streams:", &mut self.streams_input, right_x, y, 21, buf, config);
                y += 1;

                // Row 13: blank | Show Timestamps checkbox
                self.render_checkbox(60, "Show Timestamps", self.show_timestamps, right_x, y, buf);
                y += 1;
            },

            "tabbed" => {
                // Tab Active Color (10 chars, left) | Edit Tabs button (right)
                Self::render_inline_textarea_with_spacing(self.focused_field, 24, "Tab Active Color:", &mut self.text_color_input, left_x, y, 10, 3, buf, config);
                // Color preview for tab active color
                let active_preview_x = left_x + 17 + 3 + 10 + 1;
                self.render_color_preview(&self.text_color_input.lines()[0].to_string(), active_preview_x, y, buf, config);
                self.render_button(26, "Edit Tabs", right_x, y, buf);
                y += 1;

                // Tab Inactive Color (10 chars, left) | Tab Bar Position dropdown (right)
                Self::render_inline_textarea(self.focused_field, 25, "Tab Inactive Color:", &mut self.bar_color_input, left_x, y, 10, buf, config);
                // Color preview for tab inactive color
                let inactive_preview_x = left_x + 19 + 10 + 2;
                self.render_color_preview(&self.bar_color_input.lines()[0].to_string(), inactive_preview_x, y, buf, config);
                let tb = if TAB_BAR_POSITIONS.is_empty() { "" } else { TAB_BAR_POSITIONS[self.tab_bar_position_index.min(TAB_BAR_POSITIONS.len() - 1)] };
                self.render_dropdown(28, "Tab Bar Pos:", tb, right_x, y, buf);
                y += 1;

                // Tab Unread Color (10 chars, left) | New Msg input (10 chars, right)
                Self::render_inline_textarea_with_spacing(self.focused_field, 27, "Tab Unread Color:", &mut self.bar_bg_color_input, left_x, y, 10, 3, buf, config);
                // Color preview for tab unread color
                let unread_preview_x = left_x + 17 + 3 + 10 + 1;
                self.render_color_preview(&self.bar_bg_color_input.lines()[0].to_string(), unread_preview_x, y, buf, config);
                Self::render_inline_textarea_with_spacing(self.focused_field, 29, "New Msg:", &mut self.tab_unread_prefix_input, right_x, y, 10, 1, buf, config);
                y += 1;
            },

            "progress" => {
                // Text Color (10 chars, left) | Progress ID (17 chars, right)
                Self::render_inline_textarea_with_spacing(self.focused_field, 30, "Text Color:", &mut self.text_color_input, left_x, y, 10, 3, buf, config);
                Self::render_inline_textarea_with_spacing(self.focused_field, 33, "Progress ID:", &mut self.progress_id_input, right_x, y, 17, 1, buf, config);
                // Color preview for text color
                let text_preview_x = left_x + 11 + 3 + 10 + 2;
                self.render_color_preview(&self.text_color_input.lines()[0].to_string(), text_preview_x, y, buf, config);
                y += 1;

                // Bar Color (10 chars, left)
                Self::render_inline_textarea_with_spacing(self.focused_field, 31, "Bar Color:", &mut self.bar_color_input, left_x, y, 10, 4, buf, config);
                // Color preview for bar color
                let bar_preview_x = left_x + 10 + 4 + 10 + 2;
                self.render_color_preview(&self.bar_color_input.lines()[0].to_string(), bar_preview_x, y, buf, config);
                y += 1;

                // Bar BG Color (10 chars, left)
                Self::render_inline_textarea(self.focused_field, 32, "Bar BG Color:", &mut self.bar_bg_color_input, left_x, y, 10, buf, config);
                // Color preview for bar bg color
                let bar_bg_preview_x = left_x + 13 + 1 + 10 + 2;
                self.render_color_preview(&self.bar_bg_color_input.lines()[0].to_string(), bar_bg_preview_x, y, buf, config);
                y += 1;

                // Numbers Only checkbox (field 59)
                self.render_checkbox(59, "Numbers Only", self.numbers_only, left_x, y, buf);
                y += 1;
            },

            "countdown" => {
                // Bar Color (10 chars, left) | Icon (17 chars, right) - SWAPPED ORDER
                Self::render_inline_textarea_with_spacing(self.focused_field, 34, "Bar Color:", &mut self.bar_color_input, left_x, y, 10, 4, buf, config);
                Self::render_inline_textarea_with_spacing(self.focused_field, 37, "Icon:", &mut self.countdown_icon_input, right_x, y, 17, 9, buf, config);
                // Color preview for bar color
                let bar_preview_x = left_x + 10 + 4 + 10 + 2;
                self.render_color_preview(&self.bar_color_input.lines()[0].to_string(), bar_preview_x, y, buf, config);
                y += 1;

                // Bar BG Color (10 chars, left) | Countdown ID (17 chars, right) - SWAPPED ORDER
                Self::render_inline_textarea(self.focused_field, 35, "Bar BG Color:", &mut self.bar_bg_color_input, left_x, y, 10, buf, config);
                Self::render_inline_textarea_with_spacing(self.focused_field, 36, "Countdown ID:", &mut self.countdown_id_input, right_x, y, 17, 1, buf, config);
                // Color preview for bar bg color
                let bar_bg_preview_x = left_x + 13 + 1 + 10 + 2;
                self.render_color_preview(&self.bar_bg_color_input.lines()[0].to_string(), bar_bg_preview_x, y, buf, config);
                y += 1;
            },

            "active_effects" => {
                // Text Color (10 chars, left) | Effect Category dropdown (right) - SWAPPED ORDER
                Self::render_inline_textarea_with_spacing(self.focused_field, 39, "Text Color:", &mut self.text_color_input, left_x, y, 10, 4, buf, config);
                let ec = if EFFECT_CATEGORIES.is_empty() { "" } else { EFFECT_CATEGORIES[self.effect_category_index.min(EFFECT_CATEGORIES.len() - 1)] };
                self.render_dropdown(40, "Effect Category:", ec, right_x, y, buf);
                // Color preview for text color
                let text_preview_x = left_x + 11 + 4 + 10 + 2;
                self.render_color_preview(&self.text_color_input.lines()[0].to_string(), text_preview_x, y, buf, config);
                y += 1;

                // Default Color (10 chars, left) - SWAPPED ORDER
                Self::render_inline_textarea(self.focused_field, 38, "Default Color:", &mut self.effect_default_color_input, left_x, y, 10, buf, config);
                // Color preview for default color
                let default_preview_x = left_x + 14 + 1 + 10 + 2;
                self.render_color_preview(&self.effect_default_color_input.lines()[0].to_string(), default_preview_x, y, buf, config);
                y += 1;
            },

            "entity" => {
                // Streams input (21 chars, right)
                Self::render_inline_textarea(self.focused_field, 23, "Streams:", &mut self.streams_input, right_x, y, 21, buf, config);
                y += 1;
            },

            "compass" => {
                // Compass Active Color (10 chars, left)
                Self::render_inline_textarea_with_spacing(self.focused_field, 41, "Active Color:", &mut self.compass_active_color_input, left_x, y, 10, 3, buf, config);
                // Color preview
                let active_preview_x = left_x + 13 + 3 + 10 + 2;
                self.render_color_preview(&self.compass_active_color_input.lines()[0].to_string(), active_preview_x, y, buf, config);
                y += 1;

                // Compass Inactive Color (10 chars, left)
                Self::render_inline_textarea(self.focused_field, 42, "Inactive Color:", &mut self.compass_inactive_color_input, left_x, y, 10, buf, config);
                // Color preview
                let inactive_preview_x = left_x + 15 + 1 + 10 + 2;
                self.render_color_preview(&self.compass_inactive_color_input.lines()[0].to_string(), inactive_preview_x, y, buf, config);
                y += 1;
            },

            "hands" | "lefthand" | "righthand" | "spellhand" => {
                // Text Color (10 chars, left) | Hand Icon (10 chars, right)
                Self::render_inline_textarea_with_spacing(self.focused_field, 43, "Text Color:", &mut self.text_color_input, left_x, y, 10, 1, buf, config);
                Self::render_inline_textarea_with_spacing(self.focused_field, 44, "Hand Icon:", &mut self.hand_icon_input, right_x, y, 10, 1, buf, config);
                // Color preview for text color
                let text_preview_x = left_x + 11 + 1 + 10 + 2;
                self.render_color_preview(&self.text_color_input.lines()[0].to_string(), text_preview_x, y, buf, config);
                y += 1;
            },

            "indicator" => {
                // Indicator Active Color (10 chars, left) | Indicator Icon (10 chars, right)
                Self::render_inline_textarea_with_spacing(self.focused_field, 45, "Active Color:", &mut self.compass_active_color_input, left_x, y, 10, 3, buf, config);
                Self::render_inline_textarea(self.focused_field, 47, "Indicator Icon:", &mut self.hand_icon_input, right_x, y, 10, buf, config);
                // Color preview for active color
                let active_preview_x = left_x + 13 + 3 + 10 + 2;
                self.render_color_preview(&self.compass_active_color_input.lines()[0].to_string(), active_preview_x, y, buf, config);
                y += 1;

                // Indicator Inactive Color (10 chars, left)
                Self::render_inline_textarea(self.focused_field, 46, "Inactive Color:", &mut self.compass_inactive_color_input, left_x, y, 10, buf, config);
                // Color preview for inactive color
                let inactive_preview_x = left_x + 15 + 1 + 10 + 2;
                self.render_color_preview(&self.compass_inactive_color_input.lines()[0].to_string(), inactive_preview_x, y, buf, config);
                y += 1;
            },

            "injury_doll" => {
                // Injury1 Color (10 chars, left) | Scar1 Color (10 chars, right)
                Self::render_inline_textarea(self.focused_field, 48, "Injury1 Color:", &mut self.text_color_input, left_x, y, 10, buf, config);
                let injury1_preview_x = left_x + 14 + 10 + 2;
                self.render_color_preview(&self.text_color_input.lines()[0].to_string(), injury1_preview_x, y, buf, config);
                Self::render_inline_textarea(self.focused_field, 51, "Scar1 Color:", &mut self.bar_color_input, right_x, y, 10, buf, config);
                let scar1_preview_x = right_x + 12 + 10 + 2;
                self.render_color_preview(&self.bar_color_input.lines()[0].to_string(), scar1_preview_x, y, buf, config);
                y += 1;

                // Injury2 Color (10 chars, left) | Scar2 Color (10 chars, right)
                Self::render_inline_textarea(self.focused_field, 49, "Injury2 Color:", &mut self.bar_bg_color_input, left_x, y, 10, buf, config);
                let injury2_preview_x = left_x + 14 + 10 + 2;
                self.render_color_preview(&self.bar_bg_color_input.lines()[0].to_string(), injury2_preview_x, y, buf, config);
                Self::render_inline_textarea(self.focused_field, 52, "Scar2 Color:", &mut self.compass_active_color_input, right_x, y, 10, buf, config);
                let scar2_preview_x = right_x + 12 + 10 + 2;
                self.render_color_preview(&self.compass_active_color_input.lines()[0].to_string(), scar2_preview_x, y, buf, config);
                y += 1;

                // Injury3 Color (10 chars, left) | Scar3 Color (10 chars, right)
                Self::render_inline_textarea(self.focused_field, 50, "Injury3 Color:", &mut self.compass_inactive_color_input, left_x, y, 10, buf, config);
                let injury3_preview_x = left_x + 14 + 10 + 2;
                self.render_color_preview(&self.compass_inactive_color_input.lines()[0].to_string(), injury3_preview_x, y, buf, config);
                Self::render_inline_textarea(self.focused_field, 53, "Scar3 Color:", &mut self.effect_default_color_input, right_x, y, 10, buf, config);
                let scar3_preview_x = right_x + 12 + 10 + 2;
                self.render_color_preview(&self.effect_default_color_input.lines()[0].to_string(), scar3_preview_x, y, buf, config);
                y += 1;

                // Default Color (10 chars, left)
                Self::render_inline_textarea(self.focused_field, 54, "Default Color:", &mut self.progress_id_input, left_x, y, 10, buf, config);
                let default_preview_x = left_x + 14 + 10 + 2;
                self.render_color_preview(&self.progress_id_input.lines()[0].to_string(), default_preview_x, y, buf, config);
                y += 1;
            },

            "dashboard" => {
                // Dashboard Layout dropdown (left) | Edit Indicators button (right)
                let dl = if DASHBOARD_LAYOUTS.is_empty() { "" } else { DASHBOARD_LAYOUTS[self.dashboard_layout_index.min(DASHBOARD_LAYOUTS.len() - 1)] };
                self.render_dropdown(55, "Layout:", dl, left_x, y, buf);
                self.render_button(56, "Edit Indicators", right_x, y, buf);
                y += 1;

                // Dashboard Spacing (8 chars, left) | Hide Inactive checkbox (right)
                Self::render_inline_textarea(self.focused_field, 57, "Spacing:", &mut self.dashboard_spacing_input, left_x, y, 8, buf, config);
                self.render_checkbox(58, "Hide Inactive", self.dashboard_hide_inactive, right_x, y, buf);
                y += 1;
            },

            "command_input" => {
                // No additional widget-specific fields
            },

            _ => {
                // Unknown widget type - no specific fields
            }
        }
    }

    fn render_tab_editor(&mut self, area: Rect, buf: &mut Buffer) {
        let y = area.y;
        let x = area.x;

        let title = Paragraph::new("Tab Editor")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        title.render(Rect { x, y, width: area.width, height: 1 }, buf);

        match self.tab_editor.mode {
            TabEditMode::Browsing => {
                // Show tabs list in 3-column format
                if let Some(ref tabs) = self.current_window.tabs {
                    for (i, tab) in tabs.iter().enumerate().take(10) {
                        let prefix = if i == self.tab_editor.selected_index { "> " } else { "  " };
                        // Format: "  Name                  ->  stream"
                        // Name padded to 20 chars, then " -> ", then stream
                        let name_padded = format!("{:<20}", tab.name);
                        let text = format!("{}{}  ->  {}", prefix, name_padded, tab.stream);
                        let style = if i == self.tab_editor.selected_index {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let para = Paragraph::new(text).style(style);
                        para.render(Rect { x, y: y + 2 + i as u16, width: area.width, height: 1 }, buf);
                    }
                }
            },
            TabEditMode::Adding | TabEditMode::Editing => {
                // Show input form
                let mode_label = if self.tab_editor.mode == TabEditMode::Adding { "Add Tab" } else { "Edit Tab" };
                buf.set_string(x, y + 2, mode_label, Style::default().fg(Color::Yellow));

                // Tab Name input (highlight if focused)
                let name_style = if self.tab_editor.focused_input == 0 {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };
                buf.set_string(x, y + 4, "Tab Name:", name_style);
                self.tab_editor.tab_name_input.set_cursor_line_style(Style::default());
                self.tab_editor.tab_name_input.render(Rect { x: x + 11, y: y + 4, width: 30, height: 1 }, buf);

                // Tab Stream input (highlight if focused)
                let stream_style = if self.tab_editor.focused_input == 1 {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };
                buf.set_string(x, y + 6, "Stream:", stream_style);
                self.tab_editor.tab_stream_input.set_cursor_line_style(Style::default());
                self.tab_editor.tab_stream_input.render(Rect { x: x + 11, y: y + 6, width: 30, height: 1 }, buf);

                // Show Timestamps checkbox
                let checkbox_style = if self.tab_editor.focused_input == 2 {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };
                let checkbox_str = if self.tab_editor.show_timestamps { "[✓] Show Timestamps" } else { "[ ] Show Timestamps" };
                buf.set_string(x, y + 8, checkbox_str, checkbox_style);

                // Instructions
                buf.set_string(x, y + 10, "Tab: Switch Field | Space: Toggle | Enter: Save | Esc: Cancel", Style::default().fg(Color::Yellow));
            }
        }
    }

    fn render_indicator_editor(&self, area: Rect, buf: &mut Buffer) {
        let y = area.y;
        let x = area.x;

        let title = Paragraph::new("Indicator Editor")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        title.render(Rect { x, y, width: area.width, height: 1 }, buf);

        if self.indicator_editor.mode == IndicatorEditMode::Picking {
            // Show picker
            for (i, (id, icon)) in AVAILABLE_INDICATORS.iter().enumerate().take(10) {
                let prefix = if i == self.indicator_editor.picker_selected_index { "> " } else { "  " };
                let text = format!("{}{} {}", prefix, icon, id);
                let style = if i == self.indicator_editor.picker_selected_index {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                let para = Paragraph::new(text).style(style);
                para.render(Rect { x, y: y + 2 + i as u16, width: area.width, height: 1 }, buf);
            }
        } else {
            // Show current indicators
            if let Some(ref indicators) = self.current_window.dashboard_indicators {
                for (i, ind) in indicators.iter().enumerate().take(10) {
                    let prefix = if i == self.indicator_editor.selected_index { "> " } else { "  " };
                    let text = format!("{}{} {}", prefix, ind.icon, ind.id);
                    let style = if i == self.indicator_editor.selected_index {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let para = Paragraph::new(text).style(style);
                    para.render(Rect { x, y: y + 2 + i as u16, width: area.width, height: 1 }, buf);
                }
            }
        }
    }

    fn render_inline_textarea_with_spacing(focused_field: usize, field_id: usize, label: &str, textarea: &mut TextArea, x: u16, y: u16, width: usize, spaces_after_label: usize, buf: &mut Buffer, config: &crate::config::Config) {
        Self::render_inline_textarea_with_hint(focused_field, field_id, label, textarea, x, y, width, spaces_after_label, None, buf, config);
    }

    fn render_inline_textarea_with_hint(focused_field: usize, field_id: usize, label: &str, textarea: &mut TextArea, x: u16, y: u16, width: usize, spaces_after_label: usize, hint: Option<&str>, buf: &mut Buffer, config: &crate::config::Config) {
        // Render label with darker cyan, highlight when focused
        let is_focused = focused_field == field_id;
        let label_style = if is_focused {
            Style::default().fg(Color::Yellow) // Highlight when focused
        } else {
            Style::default().fg(Color::Rgb(100, 149, 237)) // Darker cyan/cornflower blue
        };

        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(label_style);
            }
        }

        // Calculate input position based on label and specified spacing
        let input_x = x + label.len() as u16 + spaces_after_label as u16;

        // Parse textarea background color from config
        let bg_color = {
            let hex = &config.colors.ui.textarea_background;
            if hex.starts_with('#') && hex.len() == 7 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[1..3], 16),
                    u8::from_str_radix(&hex[3..5], 16),
                    u8::from_str_radix(&hex[5..7], 16),
                ) {
                    Color::Rgb(r, g, b)
                } else {
                    Color::Rgb(53, 5, 5) // Fallback to dark maroon
                }
            } else {
                Color::Rgb(53, 5, 5) // Fallback to dark maroon
            }
        };
        let content = textarea.lines()[0].to_string();

        for i in 0..width {
            if let Some(cell) = buf.cell_mut((input_x + i as u16, y)) {
                if i < content.len() {
                    // Show actual content
                    cell.set_char(content.chars().nth(i).unwrap_or(' '));
                    if is_focused {
                        cell.set_style(Style::default().fg(Color::White).bg(bg_color));
                    } else {
                        cell.set_style(Style::default().fg(Color::DarkGray).bg(bg_color));
                    }
                } else if content.is_empty() && i < hint.unwrap_or("").len() {
                    // Show hint if field is empty
                    cell.set_char(hint.unwrap_or("").chars().nth(i).unwrap_or(' '));
                    cell.set_style(Style::default().fg(Color::Rgb(80, 80, 80)).bg(bg_color)); // Dark gray hint
                } else {
                    // Empty space
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(bg_color));
                }
            }
        }
    }

    fn render_inline_textarea(focused_field: usize, field_id: usize, label: &str, textarea: &mut TextArea, x: u16, y: u16, width: usize, buf: &mut Buffer, config: &crate::config::Config) {
        Self::render_inline_textarea_with_spacing(focused_field, field_id, label, textarea, x, y, width, 1, buf, config);
    }

    fn render_checkbox(&self, field_id: usize, label: &str, checked: bool, x: u16, y: u16, buf: &mut Buffer) {
        let is_focused = self.focused_field == field_id;
        let checkbox = if checked { "[✓]" } else { "[ ]" };

        // Label in darker cyan on left
        let label_style = Style::default().fg(Color::Rgb(100, 149, 237));
        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(label_style);
            }
        }

        // Checkbox aligned at fixed position (x+15)
        let checkbox_x = x + 15;
        let checkbox_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Rgb(150, 150, 150)) // Darker gray instead of white
        };

        for (i, ch) in checkbox.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((checkbox_x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(checkbox_style);
            }
        }
    }

    fn render_dropdown_with_spacing(&self, field_id: usize, label: &str, value: &str, x: u16, y: u16, spaces_after_label: usize, buf: &mut Buffer) {
        let is_focused = self.focused_field == field_id;

        // Label in darker cyan
        let label_style = Style::default().fg(Color::Rgb(100, 149, 237));
        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(label_style);
            }
        }

        // Value after label with specified spacing
        let value_x = x + label.len() as u16 + spaces_after_label as u16;
        let value_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Rgb(150, 150, 150)) // Darker gray
        };

        for (i, ch) in value.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((value_x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(value_style);
            }
        }
    }

    fn render_dropdown(&self, field_id: usize, label: &str, value: &str, x: u16, y: u16, buf: &mut Buffer) {
        self.render_dropdown_with_spacing(field_id, label, value, x, y, 1, buf);
    }

    fn render_text(&self, x: u16, y: u16, text: &str, color: Color, buf: &mut Buffer) {
        let para = Paragraph::new(text).style(Style::default().fg(color));
        para.render(Rect { x, y, width: 30, height: 1 }, buf);
    }

    fn render_button(&self, field_id: usize, label: &str, x: u16, y: u16, buf: &mut Buffer) {
        let is_focused = self.focused_field == field_id;
        let style = if is_focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };

        let text = format!("[ {} ]", label);
        let para = Paragraph::new(text).style(style);
        para.render(Rect { x, y, width: 30, height: 1 }, buf);
    }

    fn render_color_preview(&self, color_str: &str, x: u16, y: u16, buf: &mut Buffer, config: &crate::config::Config) {
        // Parse hex color and show a preview box
        let preview_text = "███"; // 3 block characters for preview

        // Resolve color name to hex if it's a palette color
        let resolved_color = config.resolve_color(color_str.trim())
            .unwrap_or_else(|| color_str.trim().to_string());

        if let Some(color) = self.parse_hex_color(&resolved_color) {
            for (i, ch) in preview_text.chars().enumerate() {
                if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                    cell.set_char(ch);
                    cell.set_fg(color);
                }
            }
        }
    }

    fn parse_hex_color(&self, s: &str) -> Option<Color> {
        let s = s.trim().trim_start_matches('#');
        if s.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&s[0..2], 16),
                u8::from_str_radix(&s[2..4], 16),
                u8::from_str_radix(&s[4..6], 16),
            ) {
                return Some(Color::Rgb(r, g, b));
            }
        }
        None
    }

    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool) -> bool {
        if !self.active {
            return false;
        }

        let popup_width = 70;

        // Check if mouse is on title bar
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > self.popup_x
            && mouse_col < self.popup_x + popup_width - 1;

        if mouse_down && on_title_bar && !self.is_dragging {
            self.is_dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
            return true;
        }

        if self.is_dragging {
            if mouse_down {
                self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
                self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
                return true;
            } else {
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    pub fn close(&mut self) {
        self.active = false;
        self.is_dragging = false;
        self.popup_x = 5;
        self.popup_y = 1;
    }
}
