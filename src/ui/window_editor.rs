use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;

use crate::config::{ContentAlign, DashboardIndicatorDef, TabConfig, WindowDef};

/// Field editor types for different input methods
#[derive(Debug, Clone, PartialEq)]
pub enum FieldEditor {
    TextInput,           // Single-line text
    NumberInput,         // Numeric values
    Checkbox,            // Boolean toggle
    Dropdown(Vec<String>), // Single selection from list
    ColorPicker,         // Hex color input (#RRGGBB)
    MultiCheckbox(Vec<String>), // Multiple checkboxes (e.g., border sides)
    TagList,             // List of strings (e.g., streams)
    ColorArray(Vec<String>), // Array of colors with labels
    TabList,             // Tab management (name, stream pairs)
    DashboardIndicatorList, // Dashboard indicator editor
}

/// Field definition for window configuration
#[derive(Debug, Clone)]
pub struct WindowFieldDef {
    pub key: String,           // Internal field name
    pub label: String,         // Display label
    pub editor: FieldEditor,   // Editor type
    pub help_text: String,     // Help text shown at bottom
    pub required: bool,        // Is this field required?
    pub section: String,       // Section name for grouping
}

/// Current editing state
#[derive(Debug, Clone, PartialEq)]
pub enum EditorMode {
    SelectingWindow,     // Choosing which window to edit
    SelectingWidgetType, // Choosing widget type (for new windows)
    EditingField,        // Editing a specific field
    EditingDropdown,     // Dropdown is open for selection
    EditingMultiCheckbox, // Multi-checkbox editor is active
    EditingTagList,      // Tag list editor is active
    EditingColorArray,   // Color array editor is active
    EditingTabList,      // Tab list editor is active
    EditingDashboardIndicators, // Dashboard indicator editor is active
}

/// Window editor state
pub struct WindowEditor {
    pub active: bool,
    pub mode: EditorMode,

    // Window selection
    pub available_windows: Vec<String>,  // List of existing windows
    pub selected_window_index: usize,
    pub window_scroll_offset: usize,

    // Widget type selection (for new windows)
    pub available_widget_types: Vec<String>,
    pub selected_widget_type_index: usize,

    // Editing state
    pub is_new_window: bool,
    pub original_window: Option<WindowDef>,
    pub current_window: WindowDef,
    pub fields: Vec<WindowFieldDef>,
    pub current_field_index: usize,
    pub field_scroll_offset: usize,

    // Field-specific editing state
    pub text_input_buffer: String,
    pub text_input_cursor: usize,
    pub dropdown_selected: usize,
    pub multi_checkbox_states: HashMap<String, bool>,
    pub tag_list_items: Vec<String>,
    pub tag_list_input: String,
    pub tag_list_selected: Option<usize>,
    pub color_array_items: Vec<(String, String)>, // (label, color)
    pub color_array_selected: usize,
    pub color_array_input: String,
    pub tab_list_items: Vec<(String, String)>, // (name, stream)
    pub tab_list_selected: usize,
    pub tab_list_editing_name: bool, // true = editing name, false = editing stream
    pub tab_list_input: String,
    pub dashboard_indicators: Vec<DashboardIndicatorDef>,
    pub dashboard_indicator_selected: usize,
    pub dashboard_indicator_editing_field: usize, // 0=id, 1=icon, 2=off_color, 3=on_color
    pub dashboard_indicator_input: String,

    // Validation
    pub validation_errors: HashMap<String, String>,
    pub show_validation: bool,

    // Help text
    pub status_message: String,
}

impl WindowEditor {
    pub fn new() -> Self {
        Self {
            active: false,
            mode: EditorMode::SelectingWindow,
            available_windows: Vec::new(),
            selected_window_index: 0,
            window_scroll_offset: 0,
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
            original_window: None,
            current_window: WindowDef::default(),
            fields: Vec::new(),
            current_field_index: 0,
            field_scroll_offset: 0,
            text_input_buffer: String::new(),
            text_input_cursor: 0,
            dropdown_selected: 0,
            multi_checkbox_states: HashMap::new(),
            tag_list_items: Vec::new(),
            tag_list_input: String::new(),
            tag_list_selected: None,
            color_array_items: Vec::new(),
            color_array_selected: 0,
            color_array_input: String::new(),
            tab_list_items: Vec::new(),
            tab_list_selected: 0,
            tab_list_editing_name: true,
            tab_list_input: String::new(),
            dashboard_indicators: Vec::new(),
            dashboard_indicator_selected: 0,
            dashboard_indicator_editing_field: 0,
            dashboard_indicator_input: String::new(),
            validation_errors: HashMap::new(),
            show_validation: false,
            status_message: String::new(),
        }
    }

    /// Open editor for existing window
    pub fn open_for_window(&mut self, windows: Vec<String>, selected_window: Option<String>) {
        self.active = true;
        self.is_new_window = false;
        self.available_windows = windows;
        self.selected_window_index = 0;
        self.window_scroll_offset = 0;
        self.mode = EditorMode::SelectingWindow;

        // If a specific window was requested, select it
        if let Some(window_name) = selected_window {
            if let Some(index) = self.available_windows.iter().position(|w| w == &window_name) {
                self.selected_window_index = index;
            }
        }

        self.status_message = "Select a window to edit (↑/↓ to navigate, Enter to select, Esc to cancel)".to_string();
    }

    /// Open editor for new window
    pub fn open_for_new_window(&mut self) {
        self.active = true;
        self.is_new_window = true;
        self.mode = EditorMode::SelectingWidgetType;
        self.selected_widget_type_index = 0;
        self.status_message = "Select widget type (↑/↓ to navigate, Enter to select, Esc to cancel)".to_string();
    }

    /// Set the window being edited
    pub fn set_window(&mut self, window: WindowDef) {
        self.original_window = Some(window.clone());
        self.current_window = window;
        self.build_field_list();
        self.mode = EditorMode::EditingField;
        self.current_field_index = 0;
        self.field_scroll_offset = 0;
        self.update_status_message();
    }

    /// Build field list based on widget type
    fn build_field_list(&mut self) {
        self.fields.clear();

        let widget_type = self.current_window.widget_type.clone();

        // Common fields for all widgets
        self.add_position_fields();
        self.add_border_fields();
        self.add_appearance_fields();

        // Widget-specific fields
        match widget_type.as_str() {
            "text" => self.add_text_window_fields(),
            "tabbed" => self.add_tabbed_window_fields(),
            "active_effects" => self.add_active_effects_fields(),
            "targets" | "players" => self.add_targets_players_fields(),
            "dashboard" => self.add_dashboard_fields(),
            "indicator" => self.add_indicator_fields(),
            "compass" => self.add_compass_fields(),
            "injury_doll" => self.add_injury_doll_fields(),
            "progress" | "countdown" => self.add_progress_countdown_fields(),
            _ => {}
        }
    }

    fn add_position_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "row".to_string(),
            label: "Row".to_string(),
            editor: FieldEditor::NumberInput,
            help_text: "Starting row position (0-based)".to_string(),
            required: true,
            section: "Position & Size".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "col".to_string(),
            label: "Column".to_string(),
            editor: FieldEditor::NumberInput,
            help_text: "Starting column position (0-based)".to_string(),
            required: true,
            section: "Position & Size".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "rows".to_string(),
            label: "Height (rows)".to_string(),
            editor: FieldEditor::NumberInput,
            help_text: "Height in rows".to_string(),
            required: true,
            section: "Position & Size".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "cols".to_string(),
            label: "Width (cols)".to_string(),
            editor: FieldEditor::NumberInput,
            help_text: "Width in columns".to_string(),
            required: true,
            section: "Position & Size".to_string(),
        });
    }

    fn add_border_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "show_border".to_string(),
            label: "Show Border".to_string(),
            editor: FieldEditor::Checkbox,
            help_text: "Show window border (Space to toggle)".to_string(),
            required: false,
            section: "Border".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "border_style".to_string(),
            label: "Border Style".to_string(),
            editor: FieldEditor::Dropdown(vec![
                "single".to_string(),
                "double".to_string(),
                "rounded".to_string(),
                "thick".to_string(),
                "none".to_string(),
            ]),
            help_text: "Border style (Enter to open dropdown)".to_string(),
            required: false,
            section: "Border".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "border_sides".to_string(),
            label: "Border Sides".to_string(),
            editor: FieldEditor::MultiCheckbox(vec![
                "top".to_string(),
                "bottom".to_string(),
                "left".to_string(),
                "right".to_string(),
            ]),
            help_text: "Which sides to show border (Enter to edit, Space to toggle)".to_string(),
            required: false,
            section: "Border".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "border_color".to_string(),
            label: "Border Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Border color in hex format (#RRGGBB)".to_string(),
            required: false,
            section: "Border".to_string(),
        });
    }

    fn add_appearance_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "title".to_string(),
            label: "Title".to_string(),
            editor: FieldEditor::TextInput,
            help_text: "Window title (shown in border, defaults to window name)".to_string(),
            required: false,
            section: "Appearance".to_string(),
        });

        // Only add content_align for widgets that support it
        if matches!(self.current_window.widget_type.as_str(),
            "indicator" | "compass" | "injury_doll" | "progress" | "countdown" | "dashboard") {
            self.fields.push(WindowFieldDef {
                key: "content_align".to_string(),
                label: "Content Alignment".to_string(),
                editor: FieldEditor::Dropdown(vec![
                    "top-left".to_string(),
                    "top".to_string(),
                    "top-right".to_string(),
                    "left".to_string(),
                    "center".to_string(),
                    "right".to_string(),
                    "bottom-left".to_string(),
                    "bottom".to_string(),
                    "bottom-right".to_string(),
                ]),
                help_text: "Alignment of content within widget area".to_string(),
                required: false,
                section: "Appearance".to_string(),
            });
        }

        self.fields.push(WindowFieldDef {
            key: "transparent_background".to_string(),
            label: "Transparent Background".to_string(),
            editor: FieldEditor::Checkbox,
            help_text: "Make background transparent (Space to toggle)".to_string(),
            required: false,
            section: "Appearance".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "background_color".to_string(),
            label: "Background Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Background color in hex format (#RRGGBB)".to_string(),
            required: false,
            section: "Appearance".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "locked".to_string(),
            label: "Lock Window".to_string(),
            editor: FieldEditor::Checkbox,
            help_text: "Prevent window from being moved or resized with mouse (Space to toggle)".to_string(),
            required: false,
            section: "Appearance".to_string(),
        });
    }

    fn add_text_window_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "streams".to_string(),
            label: "Streams".to_string(),
            editor: FieldEditor::TagList,
            help_text: "Streams to route to this window (Enter to manage)".to_string(),
            required: false,
            section: "Streams".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "buffer_size".to_string(),
            label: "Buffer Size".to_string(),
            editor: FieldEditor::NumberInput,
            help_text: "Maximum number of lines to keep in buffer".to_string(),
            required: false,
            section: "Advanced".to_string(),
        });
    }

    fn add_tabbed_window_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "tab_bar_position".to_string(),
            label: "Tab Bar Position".to_string(),
            editor: FieldEditor::Dropdown(vec![
                "top".to_string(),
                "bottom".to_string(),
            ]),
            help_text: "Position of tab bar".to_string(),
            required: false,
            section: "Tabs".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "tab_active_color".to_string(),
            label: "Active Tab Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Color for active tab".to_string(),
            required: false,
            section: "Tabs".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "tab_inactive_color".to_string(),
            label: "Inactive Tab Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Color for inactive tabs".to_string(),
            required: false,
            section: "Tabs".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "tab_unread_color".to_string(),
            label: "Unread Tab Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Color for tabs with unread messages".to_string(),
            required: false,
            section: "Tabs".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "tab_unread_prefix".to_string(),
            label: "Unread Tab Prefix".to_string(),
            editor: FieldEditor::TextInput,
            help_text: "Prefix for tabs with unread (e.g., '* ')".to_string(),
            required: false,
            section: "Tabs".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "tabs".to_string(),
            label: "Tabs".to_string(),
            editor: FieldEditor::TabList,
            help_text: "Tab definitions (Enter to manage)".to_string(),
            required: true,
            section: "Tabs".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "buffer_size".to_string(),
            label: "Buffer Size".to_string(),
            editor: FieldEditor::NumberInput,
            help_text: "Maximum number of lines to keep in buffer per tab".to_string(),
            required: false,
            section: "Advanced".to_string(),
        });
    }

    fn add_active_effects_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "effect_category".to_string(),
            label: "Effect Category".to_string(),
            editor: FieldEditor::Dropdown(vec![
                "All".to_string(),
                "ActiveSpells".to_string(),
                "Buffs".to_string(),
                "Debuffs".to_string(),
                "Cooldowns".to_string(),
            ]),
            help_text: "Which effects to display".to_string(),
            required: false,
            section: "Effects".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "default_bar_color".to_string(),
            label: "Default Bar Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Default color for effect bars".to_string(),
            required: false,
            section: "Effects".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "streams".to_string(),
            label: "Streams".to_string(),
            editor: FieldEditor::TagList,
            help_text: "Streams to route to this window (Enter to manage)".to_string(),
            required: false,
            section: "Streams".to_string(),
        });
    }

    fn add_targets_players_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "streams".to_string(),
            label: "Streams".to_string(),
            editor: FieldEditor::TagList,
            help_text: "Streams to route to this window (Enter to manage)".to_string(),
            required: false,
            section: "Streams".to_string(),
        });
    }

    fn add_dashboard_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "dashboard_layout".to_string(),
            label: "Dashboard Layout".to_string(),
            editor: FieldEditor::Dropdown(vec![
                "horizontal".to_string(),
                "vertical".to_string(),
                "grid_2x2".to_string(),
                "grid_3x3".to_string(),
            ]),
            help_text: "Layout of dashboard indicators".to_string(),
            required: false,
            section: "Dashboard".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "dashboard_spacing".to_string(),
            label: "Dashboard Spacing".to_string(),
            editor: FieldEditor::NumberInput,
            help_text: "Spacing between indicators".to_string(),
            required: false,
            section: "Dashboard".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "dashboard_hide_inactive".to_string(),
            label: "Hide Inactive Indicators".to_string(),
            editor: FieldEditor::Checkbox,
            help_text: "Hide inactive indicators (Space to toggle)".to_string(),
            required: false,
            section: "Dashboard".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "dashboard_indicators".to_string(),
            label: "Indicators".to_string(),
            editor: FieldEditor::DashboardIndicatorList,
            help_text: "Dashboard indicators (Enter to manage)".to_string(),
            required: false,
            section: "Dashboard".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "streams".to_string(),
            label: "Streams".to_string(),
            editor: FieldEditor::TagList,
            help_text: "Streams to route to this window (Enter to manage)".to_string(),
            required: false,
            section: "Streams".to_string(),
        });
    }

    fn add_indicator_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "indicator_id".to_string(),
            label: "Indicator ID".to_string(),
            editor: FieldEditor::TextInput,
            help_text: "Indicator identifier (e.g., 'poisoned', 'diseased')".to_string(),
            required: true,
            section: "Indicator".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "indicator_icon".to_string(),
            label: "Icon".to_string(),
            editor: FieldEditor::TextInput,
            help_text: "Unicode icon character".to_string(),
            required: false,
            section: "Indicator".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "indicator_colors".to_string(),
            label: "Colors".to_string(),
            editor: FieldEditor::ColorArray(vec![
                "Off".to_string(),
                "On".to_string(),
            ]),
            help_text: "Indicator colors (Enter to edit)".to_string(),
            required: false,
            section: "Indicator".to_string(),
        });
    }

    fn add_compass_fields(&mut self) {
        // Compass only has common fields (position, border, appearance)
    }

    fn add_injury_doll_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "injury_colors".to_string(),
            label: "Injury Colors".to_string(),
            editor: FieldEditor::ColorArray(vec![
                "None".to_string(),
                "Injury 1".to_string(),
                "Injury 2".to_string(),
                "Injury 3".to_string(),
                "Scar 1".to_string(),
                "Scar 2".to_string(),
                "Scar 3".to_string(),
            ]),
            help_text: "Injury level colors (Enter to edit)".to_string(),
            required: false,
            section: "Colors".to_string(),
        });
    }

    fn add_progress_countdown_fields(&mut self) {
        self.fields.push(WindowFieldDef {
            key: "bar_color".to_string(),
            label: "Bar Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Color for the progress bar".to_string(),
            required: false,
            section: "Bar Colors".to_string(),
        });

        self.fields.push(WindowFieldDef {
            key: "bar_background_color".to_string(),
            label: "Bar Background Color".to_string(),
            editor: FieldEditor::ColorPicker,
            help_text: "Background color for the bar".to_string(),
            required: false,
            section: "Bar Colors".to_string(),
        });
    }

    fn update_status_message(&mut self) {
        if self.current_field_index >= self.fields.len() {
            self.status_message = "Navigation: ↑/↓ to move, Enter to edit, Ctrl+S to save, Esc to cancel".to_string();
            return;
        }

        let field = &self.fields[self.current_field_index];
        self.status_message = field.help_text.clone();
    }

    /// Get current field value as a display string
    fn get_field_value_string(&self, field: &WindowFieldDef) -> String {
        match field.key.as_str() {
            "row" => self.current_window.row.to_string(),
            "col" => self.current_window.col.to_string(),
            "rows" => self.current_window.rows.to_string(),
            "cols" => self.current_window.cols.to_string(),
            "show_border" => if self.current_window.show_border { "[X]" } else { "[ ]" }.to_string(),
            "border_style" => self.current_window.border_style.clone().unwrap_or_else(|| "none".to_string()),
            "border_sides" => {
                if let Some(sides) = &self.current_window.border_sides {
                    sides.join(", ")
                } else {
                    "all".to_string()
                }
            },
            "border_color" => self.current_window.border_color.clone().unwrap_or_else(|| "(none)".to_string()),
            "title" => self.current_window.title.clone().unwrap_or_else(|| "(default)".to_string()),
            "content_align" => self.current_window.content_align.clone().unwrap_or_else(|| "top-left".to_string()),
            "transparent_background" => if self.current_window.transparent_background { "[X]" } else { "[ ]" }.to_string(),
            "background_color" => self.current_window.background_color.clone().unwrap_or_else(|| "(none)".to_string()),
            "locked" => if self.current_window.locked { "[X]" } else { "[ ]" }.to_string(),
            "streams" => format!("{} streams", self.current_window.streams.len()),
            "buffer_size" => self.current_window.buffer_size.to_string(),
            "tab_bar_position" => self.current_window.tab_bar_position.clone().unwrap_or_else(|| "top".to_string()),
            "tab_active_color" => self.current_window.tab_active_color.clone().unwrap_or_else(|| "(default)".to_string()),
            "tab_inactive_color" => self.current_window.tab_inactive_color.clone().unwrap_or_else(|| "(default)".to_string()),
            "tab_unread_color" => self.current_window.tab_unread_color.clone().unwrap_or_else(|| "(default)".to_string()),
            "tab_unread_prefix" => self.current_window.tab_unread_prefix.clone().unwrap_or_else(|| "(none)".to_string()),
            "tabs" => format!("{} tabs", self.current_window.tabs.as_ref().map(|t| t.len()).unwrap_or(0)),
            "effect_category" => self.current_window.effect_category.clone().unwrap_or_else(|| "All".to_string()),
            "default_bar_color" => self.current_window.bar_color.clone().unwrap_or_else(|| "(default)".to_string()),
            "dashboard_layout" => self.current_window.dashboard_layout.clone().unwrap_or_else(|| "horizontal".to_string()),
            "dashboard_spacing" => self.current_window.dashboard_spacing.map(|s| s.to_string()).unwrap_or_else(|| "1".to_string()),
            "dashboard_hide_inactive" => if self.current_window.dashboard_hide_inactive.unwrap_or(false) { "[X]" } else { "[ ]" }.to_string(),
            "dashboard_indicators" => format!("{} indicators", self.current_window.dashboard_indicators.as_ref().map(|i| i.len()).unwrap_or(0)),
            "indicator_id" => "(not set)".to_string(), // TODO: Store separately
            "indicator_icon" => "(not set)".to_string(), // TODO: Store separately
            "indicator_colors" => "(not set)".to_string(), // TODO: Store separately
            "injury_colors" => "(7 colors)".to_string(), // TODO: Store separately
            "bar_color" => self.current_window.bar_color.clone().unwrap_or_else(|| "(default)".to_string()),
            "bar_background_color" => self.current_window.bar_background_color.clone().unwrap_or_else(|| "(default)".to_string()),
            _ => "".to_string(),
        }
    }

    /// Close the editor
    pub fn close(&mut self) {
        self.active = false;
        self.mode = EditorMode::SelectingWindow;
        self.original_window = None;
        self.validation_errors.clear();
        self.show_validation = false;
    }

    /// Get the edited window (if valid)
    pub fn get_window(&self) -> Option<WindowDef> {
        if self.validation_errors.is_empty() {
            Some(self.current_window.clone())
        } else {
            None
        }
    }

    /// Get the selected window name (from window selection mode)
    pub fn get_selected_window_name(&self) -> Option<String> {
        if self.mode == EditorMode::SelectingWindow && self.selected_window_index < self.available_windows.len() {
            Some(self.available_windows[self.selected_window_index].clone())
        } else {
            None
        }
    }

    /// Render the window editor
    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        match self.mode {
            EditorMode::SelectingWindow => self.render_window_selection(f, area),
            EditorMode::SelectingWidgetType => self.render_widget_type_selection(f, area),
            EditorMode::EditingField => self.render_field_editor(f, area),
            EditorMode::EditingDropdown => self.render_dropdown_editor(f, area),
            EditorMode::EditingMultiCheckbox => self.render_multi_checkbox_editor(f, area),
            EditorMode::EditingTagList => self.render_tag_list_editor(f, area),
            EditorMode::EditingColorArray => self.render_color_array_editor(f, area),
            EditorMode::EditingTabList => self.render_tab_list_editor(f, area),
            EditorMode::EditingDashboardIndicators => self.render_dashboard_indicator_editor(f, area),
        }
    }

    fn render_window_selection(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(10),    // Window list
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let title = Paragraph::new("Select Window to Edit")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Window list
        let visible_height = chunks[1].height.saturating_sub(2) as usize; // Account for borders
        let list_items: Vec<Line> = self.available_windows
            .iter()
            .enumerate()
            .skip(self.window_scroll_offset)
            .take(visible_height)
            .map(|(i, window)| {
                let style = if i == self.selected_window_index {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(window.clone(), style),
                ])
            })
            .collect();

        let list = Paragraph::new(list_items)
            .block(Block::default().borders(Borders::ALL).title("Windows").border_style(Style::default().fg(Color::White)));
        f.render_widget(list, chunks[1]);

        // Status
        let status = Paragraph::new(self.status_message.clone())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[2]);
    }

    fn render_widget_type_selection(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(10),    // Type list
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let title = Paragraph::new("Select Widget Type")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Widget type list
        let list_items: Vec<Line> = self.available_widget_types
            .iter()
            .enumerate()
            .map(|(i, widget_type)| {
                let style = if i == self.selected_widget_type_index {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(widget_type.clone(), style),
                ])
            })
            .collect();

        let list = Paragraph::new(list_items)
            .block(Block::default().borders(Borders::ALL).title("Widget Types").border_style(Style::default().fg(Color::White)));
        f.render_widget(list, chunks[1]);

        // Status
        let status = Paragraph::new(self.status_message.clone())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[2]);
    }

    fn render_field_editor(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(10),    // Field list
                Constraint::Length(5),  // Status
            ])
            .split(area);

        // Title
        let title_text = if self.is_new_window {
            format!("New Window: {}", self.current_window.widget_type)
        } else {
            format!("Edit Window: {}", self.current_window.name)
        };
        let title = Paragraph::new(title_text)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Field list
        let visible_height = chunks[1].height.saturating_sub(2) as usize;
        let mut current_section = String::new();
        let mut lines = Vec::new();

        for (i, field) in self.fields.iter().enumerate().skip(self.field_scroll_offset) {
            if lines.len() >= visible_height {
                break;
            }

            // Add section header if new section
            if field.section != current_section {
                current_section = field.section.clone();
                lines.push(Line::from(vec![
                    Span::styled(format!("─── {} ───", current_section), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                ]));
            }

            let is_selected = i == self.current_field_index;
            let value_str = self.get_field_value_string(field);

            let label_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let value_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if self.validation_errors.contains_key(&field.key) {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}: ", field.label), label_style),
                Span::styled(value_str, value_style),
            ]));
        }

        let list = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Fields").border_style(Style::default().fg(Color::White)));
        f.render_widget(list, chunks[1]);

        // Status
        let mut status_lines = vec![
            Line::from(self.status_message.clone()),
        ];

        // Show validation errors if any
        if self.show_validation && !self.validation_errors.is_empty() {
            status_lines.push(Line::from(Span::styled(
                format!("{} validation errors", self.validation_errors.len()),
                Style::default().fg(Color::Red)
            )));
        }

        status_lines.push(Line::from("Tab/Shift+Tab: Navigate | Enter: Edit | Ctrl+S: Save | Esc: Cancel"));

        let status = Paragraph::new(status_lines)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[2]);
    }

    fn render_dropdown_editor(&self, f: &mut Frame, area: Rect) {
        // Get current field to access dropdown options
        if self.current_field_index >= self.fields.len() {
            return;
        }

        let field = &self.fields[self.current_field_index];
        let options = match &field.editor {
            FieldEditor::Dropdown(opts) => opts.clone(),
            _ => return,
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(5),     // Options
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let title = Paragraph::new(format!("Select: {}", field.label))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Options list
        let list_items: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, option)| {
                let style = if i == self.dropdown_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(option.clone(), style),
                ])
            })
            .collect();

        let list = Paragraph::new(list_items)
            .block(Block::default().borders(Borders::ALL).title("Options").border_style(Style::default().fg(Color::White)));
        f.render_widget(list, chunks[1]);

        // Status
        let status = Paragraph::new("↑/↓: Navigate | Enter: Select | Esc: Cancel")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[2]);
    }

    fn render_multi_checkbox_editor(&self, f: &mut Frame, area: Rect) {
        // Get current field to access checkbox options
        if self.current_field_index >= self.fields.len() {
            return;
        }

        let field = &self.fields[self.current_field_index];
        let options = match &field.editor {
            FieldEditor::MultiCheckbox(opts) => opts.clone(),
            _ => return,
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(5),     // Options
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let title = Paragraph::new(format!("Edit: {}", field.label))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Options list with checkboxes
        let list_items: Vec<Line> = options
            .iter()
            .map(|option| {
                let checked = self.multi_checkbox_states.get(option).copied().unwrap_or(false);
                let checkbox = if checked { "[X]" } else { "[ ]" };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(checkbox, Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(option.clone(), Style::default().fg(Color::White)),
                ])
            })
            .collect();

        let list = Paragraph::new(list_items)
            .block(Block::default().borders(Borders::ALL).title("Options").border_style(Style::default().fg(Color::White)));
        f.render_widget(list, chunks[1]);

        // Status
        let status = Paragraph::new("Click to toggle | Enter: Save | Esc: Cancel")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[2]);
    }

    fn render_tag_list_editor(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(5),     // Tag list
                Constraint::Length(3),  // Input
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let field = &self.fields[self.current_field_index];
        let title = Paragraph::new(format!("Edit: {}", field.label))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Tag list
        let list_items: Vec<Line> = self.tag_list_items
            .iter()
            .enumerate()
            .map(|(i, tag)| {
                let style = if Some(i) == self.tag_list_selected {
                    Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(tag.clone(), style),
                ])
            })
            .collect();

        let list_display = if list_items.is_empty() {
            Paragraph::new("(no items)")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title("Items").border_style(Style::default().fg(Color::White)))
        } else {
            Paragraph::new(list_items)
                .block(Block::default().borders(Borders::ALL).title("Items").border_style(Style::default().fg(Color::White)))
        };
        f.render_widget(list_display, chunks[1]);

        // Input field
        let input = Paragraph::new(self.tag_list_input.clone())
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Add Item").border_style(Style::default().fg(Color::White)));
        f.render_widget(input, chunks[2]);

        // Status
        let status = Paragraph::new("Type and press Enter to add | Click item to remove | Esc: Done")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[3]);
    }

    fn render_color_array_editor(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(5),     // Color list
                Constraint::Length(3),  // Input
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let field = &self.fields[self.current_field_index];
        let title = Paragraph::new(format!("Edit: {}", field.label))
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Color list
        let list_items: Vec<Line> = self.color_array_items
            .iter()
            .enumerate()
            .map(|(i, (label, color))| {
                let style = if i == self.color_array_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{}: ", label), style),
                    Span::styled(color.clone(), Style::default().fg(Color::Green)),
                ])
            })
            .collect();

        let list = Paragraph::new(list_items)
            .block(Block::default().borders(Borders::ALL).title("Colors").border_style(Style::default().fg(Color::White)));
        f.render_widget(list, chunks[1]);

        // Input field
        let input = Paragraph::new(self.color_array_input.clone())
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Edit Color (hex)").border_style(Style::default().fg(Color::White)));
        f.render_widget(input, chunks[2]);

        // Status
        let status = Paragraph::new("↑/↓: Select | Type hex color | Enter: Save | Esc: Done")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[3]);
    }

    fn render_tab_list_editor(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(5),     // Tab list
                Constraint::Length(3),  // Input
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let title = Paragraph::new("Edit Tabs")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Tab list
        let list_items: Vec<Line> = self.tab_list_items
            .iter()
            .enumerate()
            .map(|(i, (name, stream))| {
                let style = if i == self.tab_list_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} -> {}", name, stream), style),
                ])
            })
            .collect();

        let list_display = if list_items.is_empty() {
            Paragraph::new("(no tabs - press 'a' to add)")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title("Tabs").border_style(Style::default().fg(Color::White)))
        } else {
            Paragraph::new(list_items)
                .block(Block::default().borders(Borders::ALL).title("Tabs").border_style(Style::default().fg(Color::White)))
        };
        f.render_widget(list_display, chunks[1]);

        // Input field
        let input_title = if self.tab_list_editing_name {
            "Tab Name"
        } else {
            "Stream Name"
        };
        let input = Paragraph::new(self.tab_list_input.clone())
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title(input_title).border_style(Style::default().fg(Color::White)));
        f.render_widget(input, chunks[2]);

        // Status
        let status = Paragraph::new("a: Add tab | d: Delete selected | ↑/↓: Navigate | Esc: Done")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[3]);
    }

    fn render_dashboard_indicator_editor(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(5),     // Indicator list
                Constraint::Length(5),  // Edit fields
                Constraint::Length(3),  // Status
            ])
            .split(area);

        // Title
        let title = Paragraph::new("Edit Dashboard Indicators")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        f.render_widget(title, chunks[0]);

        // Indicator list
        let list_items: Vec<Line> = self.dashboard_indicators
            .iter()
            .enumerate()
            .map(|(i, indicator)| {
                let style = if i == self.dashboard_indicator_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} {} [{}|{}]", indicator.icon, indicator.id,
                        indicator.colors.get(0).unwrap_or(&String::new()),
                        indicator.colors.get(1).unwrap_or(&String::new())), style),
                ])
            })
            .collect();

        let list_display = if list_items.is_empty() {
            Paragraph::new("(no indicators - press 'a' to add)")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title("Indicators").border_style(Style::default().fg(Color::White)))
        } else {
            Paragraph::new(list_items)
                .block(Block::default().borders(Borders::ALL).title("Indicators").border_style(Style::default().fg(Color::White)))
        };
        f.render_widget(list_display, chunks[1]);

        // Edit fields
        let field_names = ["ID", "Icon", "Off Color", "On Color"];
        let current_field_name = field_names[self.dashboard_indicator_editing_field];

        let edit_lines = vec![
            Line::from(format!("Editing: {}", current_field_name)),
            Line::from(""),
            Line::from(self.dashboard_indicator_input.clone()),
        ];

        let edit = Paragraph::new(edit_lines)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Edit Field").border_style(Style::default().fg(Color::White)));
        f.render_widget(edit, chunks[2]);

        // Status
        let status = Paragraph::new("a: Add | d: Delete | Tab: Next field | ↑/↓: Navigate | Enter: Save | Esc: Done")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::White)));
        f.render_widget(status, chunks[3]);
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.mode {
            EditorMode::SelectingWindow => self.handle_window_selection_key(key),
            EditorMode::SelectingWidgetType => self.handle_widget_type_selection_key(key),
            EditorMode::EditingField => self.handle_field_editor_key(key),
            EditorMode::EditingDropdown => self.handle_dropdown_editor_key(key),
            EditorMode::EditingMultiCheckbox => self.handle_multi_checkbox_editor_key(key),
            EditorMode::EditingTagList => self.handle_tag_list_editor_key(key),
            EditorMode::EditingColorArray => self.handle_color_array_editor_key(key),
            EditorMode::EditingTabList => self.handle_tab_list_editor_key(key),
            EditorMode::EditingDashboardIndicators => self.handle_dashboard_indicator_editor_key(key),
        }
    }

    fn handle_window_selection_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.close();
                true
            },
            KeyCode::Up => {
                if self.selected_window_index > 0 {
                    self.selected_window_index -= 1;
                    // Adjust scroll if needed
                    if self.selected_window_index < self.window_scroll_offset {
                        self.window_scroll_offset = self.selected_window_index;
                    }
                }
                true
            },
            KeyCode::Down => {
                if self.selected_window_index + 1 < self.available_windows.len() {
                    self.selected_window_index += 1;
                    // Adjust scroll if needed (simplified - actual visible height would be passed in)
                    // We'll implement proper scrolling when integrated
                }
                true
            },
            KeyCode::Enter => {
                // Transition to editing the selected window
                // The app.rs will handle loading the actual window data
                true
            },
            _ => false,
        }
    }

    fn handle_widget_type_selection_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.close();
                true
            },
            KeyCode::Up => {
                if self.selected_widget_type_index > 0 {
                    self.selected_widget_type_index -= 1;
                }
                true
            },
            KeyCode::Down => {
                if self.selected_widget_type_index + 1 < self.available_widget_types.len() {
                    self.selected_widget_type_index += 1;
                }
                true
            },
            KeyCode::Enter => {
                // Set the widget type and transition to field editing
                self.current_window.widget_type = self.available_widget_types[self.selected_widget_type_index].clone();
                self.build_field_list();
                self.mode = EditorMode::EditingField;
                self.update_status_message();
                true
            },
            _ => false,
        }
    }

    fn handle_field_editor_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.close();
                true
            },
            KeyCode::Up | KeyCode::BackTab => {
                if self.current_field_index > 0 {
                    self.current_field_index -= 1;
                    self.update_status_message();
                }
                true
            },
            KeyCode::Down | KeyCode::Tab => {
                if self.current_field_index + 1 < self.fields.len() {
                    self.current_field_index += 1;
                    self.update_status_message();
                }
                true
            },
            KeyCode::Enter => {
                // Enter editing mode for this field
                self.enter_field_editor();
                true
            },
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Save changes
                self.validate_all_fields();
                true
            },
            KeyCode::Char(' ') => {
                // Toggle checkbox fields directly
                if self.current_field_index < self.fields.len() {
                    let field_key = self.fields[self.current_field_index].key.clone();
                    if let FieldEditor::Checkbox = self.fields[self.current_field_index].editor {
                        self.toggle_checkbox_field(&field_key);
                    }
                }
                true
            },
            _ => false,
        }
    }

    fn enter_field_editor(&mut self) {
        if self.current_field_index >= self.fields.len() {
            return;
        }

        let field = self.fields[self.current_field_index].clone();

        match &field.editor {
            FieldEditor::TextInput | FieldEditor::NumberInput | FieldEditor::ColorPicker => {
                // Start inline text editing
                self.text_input_buffer = self.get_field_value_string(&field);
                if self.text_input_buffer == "(none)" || self.text_input_buffer == "(default)" {
                    self.text_input_buffer.clear();
                }
                self.text_input_cursor = self.text_input_buffer.len();
                // Stay in EditingField mode but handle text input
            },
            FieldEditor::Checkbox => {
                // Checkboxes toggle with space, enter does nothing
            },
            FieldEditor::Dropdown(options) => {
                // Find current value in dropdown
                let current_value = self.get_field_value_string(&field);
                self.dropdown_selected = options.iter().position(|o| o == &current_value).unwrap_or(0);
                self.mode = EditorMode::EditingDropdown;
            },
            FieldEditor::MultiCheckbox(options) => {
                // Initialize checkbox states
                self.multi_checkbox_states.clear();
                if let Some(sides) = &self.current_window.border_sides {
                    for side in sides {
                        self.multi_checkbox_states.insert(side.clone(), true);
                    }
                }
                self.mode = EditorMode::EditingMultiCheckbox;
            },
            FieldEditor::TagList => {
                // Load current tags
                self.tag_list_items = self.current_window.streams.clone();
                self.tag_list_input.clear();
                self.tag_list_selected = None;
                self.mode = EditorMode::EditingTagList;
            },
            FieldEditor::ColorArray(labels) => {
                // Load current colors (this needs proper implementation based on field)
                self.color_array_items = labels.iter().map(|l| (l.clone(), "#ffffff".to_string())).collect();
                self.color_array_selected = 0;
                self.color_array_input.clear();
                self.mode = EditorMode::EditingColorArray;
            },
            FieldEditor::TabList => {
                // Load current tabs
                self.tab_list_items = self.current_window.tabs.as_ref()
                    .map(|tabs| tabs.iter().map(|t| (t.name.clone(), t.stream.clone())).collect())
                    .unwrap_or_default();
                self.tab_list_selected = 0;
                self.tab_list_input.clear();
                self.tab_list_editing_name = true;
                self.mode = EditorMode::EditingTabList;
            },
            FieldEditor::DashboardIndicatorList => {
                // Load current indicators
                self.dashboard_indicators = self.current_window.dashboard_indicators.clone().unwrap_or_default();
                self.dashboard_indicator_selected = 0;
                self.dashboard_indicator_editing_field = 0;
                self.dashboard_indicator_input.clear();
                self.mode = EditorMode::EditingDashboardIndicators;
            },
        }
    }

    fn toggle_checkbox_field(&mut self, key: &str) {
        match key {
            "show_border" => self.current_window.show_border = !self.current_window.show_border,
            "transparent_background" => self.current_window.transparent_background = !self.current_window.transparent_background,
            "locked" => self.current_window.locked = !self.current_window.locked,
            "dashboard_hide_inactive" => {
                let current = self.current_window.dashboard_hide_inactive.unwrap_or(false);
                self.current_window.dashboard_hide_inactive = Some(!current);
            },
            _ => {}
        }
    }

    fn handle_dropdown_editor_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.mode = EditorMode::EditingField;
                true
            },
            KeyCode::Up => {
                if self.dropdown_selected > 0 {
                    self.dropdown_selected -= 1;
                }
                true
            },
            KeyCode::Down => {
                let field = &self.fields[self.current_field_index];
                if let FieldEditor::Dropdown(options) = &field.editor {
                    if self.dropdown_selected + 1 < options.len() {
                        self.dropdown_selected += 1;
                    }
                }
                true
            },
            KeyCode::Enter => {
                // Save selection
                let field = &self.fields[self.current_field_index].clone();
                if let FieldEditor::Dropdown(options) = &field.editor {
                    let selected_value = options[self.dropdown_selected].clone();
                    self.set_field_value(&field.key, selected_value);
                }
                self.mode = EditorMode::EditingField;
                true
            },
            _ => false,
        }
    }

    fn handle_multi_checkbox_editor_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                // Save checkbox states
                let selected: Vec<String> = self.multi_checkbox_states.iter()
                    .filter(|(_, &checked)| checked)
                    .map(|(k, _)| k.clone())
                    .collect();
                self.current_window.border_sides = if selected.is_empty() {
                    None
                } else {
                    Some(selected)
                };
                self.mode = EditorMode::EditingField;
                true
            },
            _ => false,
        }
    }

    fn handle_tag_list_editor_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                // Save tags and exit
                self.current_window.streams = self.tag_list_items.clone();
                self.mode = EditorMode::EditingField;
                true
            },
            KeyCode::Enter => {
                // Add tag if input is not empty
                if !self.tag_list_input.is_empty() {
                    self.tag_list_items.push(self.tag_list_input.clone());
                    self.tag_list_input.clear();
                }
                true
            },
            KeyCode::Backspace => {
                self.tag_list_input.pop();
                true
            },
            KeyCode::Char(c) => {
                self.tag_list_input.push(c);
                true
            },
            _ => false,
        }
    }

    fn handle_color_array_editor_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                // Save colors and exit (need proper field-specific saving)
                self.mode = EditorMode::EditingField;
                true
            },
            KeyCode::Up => {
                if self.color_array_selected > 0 {
                    self.color_array_selected -= 1;
                }
                true
            },
            KeyCode::Down => {
                if self.color_array_selected + 1 < self.color_array_items.len() {
                    self.color_array_selected += 1;
                }
                true
            },
            KeyCode::Enter => {
                // Save current color
                if self.color_array_selected < self.color_array_items.len() && !self.color_array_input.is_empty() {
                    self.color_array_items[self.color_array_selected].1 = self.color_array_input.clone();
                    self.color_array_input.clear();
                }
                true
            },
            KeyCode::Backspace => {
                self.color_array_input.pop();
                true
            },
            KeyCode::Char(c) => {
                self.color_array_input.push(c);
                true
            },
            _ => false,
        }
    }

    fn handle_tab_list_editor_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                // Save tabs and exit
                self.current_window.tabs = Some(self.tab_list_items.iter()
                    .map(|(name, stream)| TabConfig { name: name.clone(), stream: stream.clone() })
                    .collect());
                self.mode = EditorMode::EditingField;
                true
            },
            KeyCode::Char('a') if self.tab_list_input.is_empty() => {
                // Start adding new tab
                self.tab_list_editing_name = true;
                true
            },
            KeyCode::Char('d') if self.tab_list_input.is_empty() => {
                // Delete selected tab
                if self.tab_list_selected < self.tab_list_items.len() {
                    self.tab_list_items.remove(self.tab_list_selected);
                    if self.tab_list_selected > 0 && self.tab_list_selected >= self.tab_list_items.len() {
                        self.tab_list_selected = self.tab_list_items.len().saturating_sub(1);
                    }
                }
                true
            },
            KeyCode::Up if self.tab_list_input.is_empty() => {
                if self.tab_list_selected > 0 {
                    self.tab_list_selected -= 1;
                }
                true
            },
            KeyCode::Down if self.tab_list_input.is_empty() => {
                if self.tab_list_selected + 1 < self.tab_list_items.len() {
                    self.tab_list_selected += 1;
                }
                true
            },
            KeyCode::Enter if !self.tab_list_input.is_empty() => {
                // Save current input and move to next field or add tab
                if self.tab_list_editing_name {
                    // Save name, move to stream
                    self.tab_list_editing_name = false;
                } else {
                    // Save stream, add tab
                    // For now, simplified - actual implementation would need to track the name
                    self.tab_list_editing_name = true;
                    self.tab_list_input.clear();
                }
                true
            },
            KeyCode::Backspace if !self.tab_list_input.is_empty() => {
                self.tab_list_input.pop();
                true
            },
            KeyCode::Char(c) if !self.tab_list_input.is_empty() || c != 'a' && c != 'd' => {
                self.tab_list_input.push(c);
                true
            },
            _ => false,
        }
    }

    fn handle_dashboard_indicator_editor_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                // Save indicators and exit
                self.current_window.dashboard_indicators = Some(self.dashboard_indicators.clone());
                self.mode = EditorMode::EditingField;
                true
            },
            KeyCode::Char('a') if self.dashboard_indicator_input.is_empty() => {
                // Add new indicator
                self.dashboard_indicators.push(DashboardIndicatorDef {
                    id: "new".to_string(),
                    icon: "?".to_string(),
                    colors: vec!["#808080".to_string(), "#00ff00".to_string()],
                });
                self.dashboard_indicator_selected = self.dashboard_indicators.len() - 1;
                true
            },
            KeyCode::Char('d') if self.dashboard_indicator_input.is_empty() => {
                // Delete selected indicator
                if self.dashboard_indicator_selected < self.dashboard_indicators.len() {
                    self.dashboard_indicators.remove(self.dashboard_indicator_selected);
                    if self.dashboard_indicator_selected > 0 && self.dashboard_indicator_selected >= self.dashboard_indicators.len() {
                        self.dashboard_indicator_selected = self.dashboard_indicators.len().saturating_sub(1);
                    }
                }
                true
            },
            KeyCode::Up if self.dashboard_indicator_input.is_empty() => {
                if self.dashboard_indicator_selected > 0 {
                    self.dashboard_indicator_selected -= 1;
                }
                true
            },
            KeyCode::Down if self.dashboard_indicator_input.is_empty() => {
                if self.dashboard_indicator_selected + 1 < self.dashboard_indicators.len() {
                    self.dashboard_indicator_selected += 1;
                }
                true
            },
            KeyCode::Tab if self.dashboard_indicator_input.is_empty() => {
                // Next field
                self.dashboard_indicator_editing_field = (self.dashboard_indicator_editing_field + 1) % 4;
                true
            },
            KeyCode::Enter if !self.dashboard_indicator_input.is_empty() => {
                // Save current field
                if self.dashboard_indicator_selected < self.dashboard_indicators.len() {
                    let indicator = &mut self.dashboard_indicators[self.dashboard_indicator_selected];
                    match self.dashboard_indicator_editing_field {
                        0 => indicator.id = self.dashboard_indicator_input.clone(),
                        1 => indicator.icon = self.dashboard_indicator_input.clone(),
                        2 => {
                            if indicator.colors.is_empty() {
                                indicator.colors.push(self.dashboard_indicator_input.clone());
                            } else {
                                indicator.colors[0] = self.dashboard_indicator_input.clone();
                            }
                        },
                        3 => {
                            if indicator.colors.len() < 2 {
                                indicator.colors.push(self.dashboard_indicator_input.clone());
                            } else {
                                indicator.colors[1] = self.dashboard_indicator_input.clone();
                            }
                        },
                        _ => {}
                    }
                    self.dashboard_indicator_input.clear();
                }
                true
            },
            KeyCode::Backspace => {
                self.dashboard_indicator_input.pop();
                true
            },
            KeyCode::Char(c) => {
                self.dashboard_indicator_input.push(c);
                true
            },
            _ => false,
        }
    }

    fn set_field_value(&mut self, key: &str, value: String) {
        match key {
            "border_style" => self.current_window.border_style = Some(value),
            "content_align" => self.current_window.content_align = Some(value),
            "tab_bar_position" => self.current_window.tab_bar_position = Some(value),
            "effect_category" => self.current_window.effect_category = Some(value),
            "dashboard_layout" => self.current_window.dashboard_layout = Some(value),
            _ => {}
        }
    }

    fn validate_all_fields(&mut self) {
        self.validation_errors.clear();

        // Validate required fields
        for field in &self.fields {
            if field.required {
                let value = self.get_field_value_string(field);
                if value.is_empty() || value == "(none)" || value == "(default)" {
                    self.validation_errors.insert(field.key.clone(), "Required field".to_string());
                }
            }
        }

        // Validate colors
        for field in &self.fields {
            if matches!(field.editor, FieldEditor::ColorPicker) {
                let value = self.get_field_value_string(field);
                if !value.is_empty() && value != "(none)" && value != "(default)" {
                    if !value.starts_with('#') || value.len() != 7 {
                        self.validation_errors.insert(field.key.clone(), "Invalid hex color (use #RRGGBB format)".to_string());
                    }
                }
            }
        }

        self.show_validation = true;
    }
}

impl Default for WindowEditor {
    fn default() -> Self {
        Self::new()
    }
}
