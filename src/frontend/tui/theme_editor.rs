//! Interactive theme editor used to author custom `AppTheme` files.
//!
//! Presents meta fields plus grouped color sections, supports dragging, and
//! serializes/deserializes `ThemeData` structs for persistence.

use crate::frontend::tui::crossterm_bridge;
use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget as RatatuiWidget,
};
use tui_textarea::TextArea;

// Re-export the shared ThemeData from the theme module
pub use crate::theme::loader::ThemeData;

/// Result of theme editor form submission
#[derive(Debug, Clone)]
pub enum ThemeEditorResult {
    Save(ThemeData),
    Cancel,
}

pub struct ThemeEditor {
    // Basic fields
    name: TextArea<'static>,
    description: TextArea<'static>,

    // Color sections with editable fields
    color_sections: Vec<ColorSection>,

    // Current section being edited (0=meta, 1+=color sections)
    current_section: usize,

    // Current field within section
    current_field: usize,

    // Position
    popup_x: u16,
    popup_y: u16,
}

struct ColorSection {
    name: &'static str,
    fields: Vec<ColorFieldEditor>,
}

struct ColorFieldEditor {
    label: &'static str,
    field_name: &'static str, // Maps to ThemeData field name
    textarea: TextArea<'static>,
}

impl ThemeEditor {
    pub fn new() -> Self {
        Self::new_create()
    }

    pub fn new_create() -> Self {
        let theme_data = ThemeData::default();

        let mut name = TextArea::default();
        name.set_placeholder_text("My Custom Theme");

        let mut description = TextArea::default();
        description.set_placeholder_text("A beautiful custom theme");

        let color_sections = Self::build_color_sections(&theme_data);

        Self {
            name,
            description,
            color_sections,
            current_section: 0,
            current_field: 0,
            popup_x: 0,
            popup_y: 0,
        }
    }

    pub fn new_edit(theme: &crate::theme::AppTheme) -> Self {
        let theme_data = ThemeData::from_theme(theme);

        let mut name = TextArea::default();
        name.insert_str(&theme.name);

        let mut description = TextArea::default();
        description.insert_str(&theme.description);

        let color_sections = Self::build_color_sections(&theme_data);

        Self {
            name,
            description,
            color_sections,
            current_section: 0,
            current_field: 0,
            popup_x: 0,
            popup_y: 0,
        }
    }

    fn build_color_sections(theme_data: &ThemeData) -> Vec<ColorSection> {
        // Helper macro to create a color field with its current value
        macro_rules! color_field {
            ($label:expr, $field_name:expr, $value:expr) => {{
                let mut textarea = TextArea::default();
                textarea.insert_str($value);
                ColorFieldEditor {
                    label: $label,
                    field_name: $field_name,
                    textarea,
                }
            }};
        }

        vec![
            ColorSection {
                name: "Window Colors",
                fields: vec![
                    color_field!("Border", "window_border", &theme_data.window_border),
                    color_field!(
                        "Border (Focused)",
                        "window_border_focused",
                        &theme_data.window_border_focused
                    ),
                    color_field!(
                        "Background",
                        "window_background",
                        &theme_data.window_background
                    ),
                    color_field!("Title", "window_title", &theme_data.window_title),
                ],
            },
            ColorSection {
                name: "Text Colors",
                fields: vec![
                    color_field!("Primary", "text_primary", &theme_data.text_primary),
                    color_field!("Secondary", "text_secondary", &theme_data.text_secondary),
                    color_field!("Disabled", "text_disabled", &theme_data.text_disabled),
                    color_field!("Selected", "text_selected", &theme_data.text_selected),
                ],
            },
            ColorSection {
                name: "Browser Colors",
                fields: vec![
                    color_field!("Border", "browser_border", &theme_data.browser_border),
                    color_field!("Title", "browser_title", &theme_data.browser_title),
                    color_field!(
                        "Item Normal",
                        "browser_item_normal",
                        &theme_data.browser_item_normal
                    ),
                    color_field!(
                        "Item Selected",
                        "browser_item_selected",
                        &theme_data.browser_item_selected
                    ),
                    color_field!(
                        "Item Focused",
                        "browser_item_focused",
                        &theme_data.browser_item_focused
                    ),
                    color_field!(
                        "Background",
                        "browser_background",
                        &theme_data.browser_background
                    ),
                    color_field!(
                        "Scrollbar",
                        "browser_scrollbar",
                        &theme_data.browser_scrollbar
                    ),
                ],
            },
            ColorSection {
                name: "Form Colors",
                fields: vec![
                    color_field!("Border", "form_border", &theme_data.form_border),
                    color_field!("Label", "form_label", &theme_data.form_label),
                    color_field!(
                        "Label (Focused)",
                        "form_label_focused",
                        &theme_data.form_label_focused
                    ),
                    color_field!(
                        "Field Background",
                        "form_field_background",
                        &theme_data.form_field_background
                    ),
                    color_field!("Field Text", "form_field_text", &theme_data.form_field_text),
                    color_field!(
                        "Checkbox Checked",
                        "form_checkbox_checked",
                        &theme_data.form_checkbox_checked
                    ),
                    color_field!(
                        "Checkbox Unchecked",
                        "form_checkbox_unchecked",
                        &theme_data.form_checkbox_unchecked
                    ),
                    color_field!("Error", "form_error", &theme_data.form_error),
                ],
            },
            ColorSection {
                name: "Editor Colors",
                fields: vec![
                    color_field!("Border", "editor_border", &theme_data.editor_border),
                    color_field!("Label", "editor_label", &theme_data.editor_label),
                    color_field!(
                        "Label (Focused)",
                        "editor_label_focused",
                        &theme_data.editor_label_focused
                    ),
                    color_field!("Text", "editor_text", &theme_data.editor_text),
                    color_field!("Cursor", "editor_cursor", &theme_data.editor_cursor),
                    color_field!("Status", "editor_status", &theme_data.editor_status),
                    color_field!(
                        "Background",
                        "editor_background",
                        &theme_data.editor_background
                    ),
                ],
            },
            ColorSection {
                name: "Menu Colors",
                fields: vec![
                    color_field!("Border", "menu_border", &theme_data.menu_border),
                    color_field!("Background", "menu_background", &theme_data.menu_background),
                    color_field!(
                        "Item Normal",
                        "menu_item_normal",
                        &theme_data.menu_item_normal
                    ),
                    color_field!(
                        "Item Selected",
                        "menu_item_selected",
                        &theme_data.menu_item_selected
                    ),
                    color_field!("Separator", "menu_separator", &theme_data.menu_separator),
                ],
            },
            ColorSection {
                name: "Button Colors",
                fields: vec![
                    color_field!("Normal", "button_normal", &theme_data.button_normal),
                    color_field!("Disabled", "button_disabled", &theme_data.button_disabled),
                ],
            },
            ColorSection {
                name: "Status Colors",
                fields: vec![color_field!(
                    "Background",
                    "status_background",
                    &theme_data.status_background
                )],
            },
        ]
    }

    pub fn handle_input(&mut self, key_event: KeyEvent) -> Option<ThemeEditorResult> {
        // Note: All navigation keys (Tab, BackTab, Esc, Ctrl+Enter, Up, Down, Ctrl+S)
        // are now routed via MenuAction in mod.rs. This method only handles text input.

        // Forward to active field (convert KeyEvent for tui-textarea compatibility)
        let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key_event);
        if self.current_section == 0 {
            // Meta fields (name/description)
            if self.current_field == 0 {
                self.name.input(rt_key);
            } else if self.current_field == 1 {
                self.description.input(rt_key);
            }
        } else {
            // Color fields
            let section_idx = self.current_section - 1;
            if let Some(section) = self.color_sections.get_mut(section_idx) {
                if let Some(field) = section.fields.get_mut(self.current_field) {
                    field.textarea.input(rt_key);
                }
            }
        }

        None
    }

    /// Handle MenuAction (called from mod.rs input routing)
    pub fn handle_action(&mut self, action: crate::core::menu_actions::MenuAction) -> Option<ThemeEditorResult> {
        use crate::core::menu_actions::MenuAction;

        match action {
            MenuAction::NavigateUp => {
                // Up arrow - navigate to previous field
                self.previous_field();
                None
            }
            MenuAction::NavigateDown => {
                // Down arrow - navigate to next field
                self.next_field();
                None
            }
            MenuAction::Save => {
                // Ctrl+S - save theme
                Some(ThemeEditorResult::Save(self.collect_theme_data()))
            }
            _ => None
        }
    }

    /// Collect all edited values into a ThemeData struct
    fn collect_theme_data(&self) -> ThemeData {
        let mut data = ThemeData::default();

        // Set name and description
        data.name = self
            .name
            .lines().first()
            .map(|s| s.to_string())
            .unwrap_or_default();
        data.description = self
            .description
            .lines().first()
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Collect all color fields
        for section in &self.color_sections {
            for field in &section.fields {
                let value = field
                    .textarea
                    .lines().first()
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Use field_name to set the correct field in ThemeData
                match field.field_name {
                    "window_border" => data.window_border = value,
                    "window_border_focused" => data.window_border_focused = value,
                    "window_background" => data.window_background = value,
                    "window_title" => data.window_title = value,
                    "text_primary" => data.text_primary = value,
                    "text_secondary" => data.text_secondary = value,
                    "text_disabled" => data.text_disabled = value,
                    "text_selected" => data.text_selected = value,
                    "browser_border" => data.browser_border = value,
                    "browser_title" => data.browser_title = value,
                    "browser_item_normal" => data.browser_item_normal = value,
                    "browser_item_selected" => data.browser_item_selected = value,
                    "browser_item_focused" => data.browser_item_focused = value,
                    "browser_background" => data.browser_background = value,
                    "browser_scrollbar" => data.browser_scrollbar = value,
                    "form_border" => data.form_border = value,
                    "form_label" => data.form_label = value,
                    "form_label_focused" => data.form_label_focused = value,
                    "form_field_background" => data.form_field_background = value,
                    "form_field_text" => data.form_field_text = value,
                    "form_checkbox_checked" => data.form_checkbox_checked = value,
                    "form_checkbox_unchecked" => data.form_checkbox_unchecked = value,
                    "form_error" => data.form_error = value,
                    "editor_border" => data.editor_border = value,
                    "editor_label" => data.editor_label = value,
                    "editor_label_focused" => data.editor_label_focused = value,
                    "editor_text" => data.editor_text = value,
                    "editor_cursor" => data.editor_cursor = value,
                    "editor_status" => data.editor_status = value,
                    "editor_background" => data.editor_background = value,
                    "menu_border" => data.menu_border = value,
                    "menu_background" => data.menu_background = value,
                    "menu_item_normal" => data.menu_item_normal = value,
                    "menu_item_selected" => data.menu_item_selected = value,
                    "menu_separator" => data.menu_separator = value,
                    "button_normal" => data.button_normal = value,
                    "button_disabled" => data.button_disabled = value,
                    "status_background" => data.status_background = value,
                    _ => {} // Ignore unknown fields
                }
            }
        }

        data
    }

    /// Jump to a specific section (0 = meta, 1+ = color sections)
    pub fn jump_to_section(&mut self, section: usize) {
        if section <= self.color_sections.len() {
            self.current_section = section;
            self.current_field = 0;
        }
    }

    pub fn next_field(&mut self) {
        if self.current_section == 0 {
            // Meta section (name, description)
            if self.current_field == 0 {
                self.current_field = 1; // Move from name to description
            } else {
                // Move to first color section
                self.current_section = 1;
                self.current_field = 0;
            }
        } else {
            // Color sections
            let section_idx = self.current_section - 1;
            if let Some(section) = self.color_sections.get(section_idx) {
                self.current_field += 1;
                if self.current_field >= section.fields.len() {
                    // Move to next section
                    self.current_section += 1;
                    self.current_field = 0;

                    // Wrap around if we've gone past the last section
                    if self.current_section > self.color_sections.len() {
                        self.current_section = 0;
                        self.current_field = 0;
                    }
                }
            }
        }
    }

    pub fn previous_field(&mut self) {
        if self.current_field > 0 {
            self.current_field -= 1;
        } else {
            // Move to previous section
            if self.current_section > 0 {
                self.current_section -= 1;

                if self.current_section == 0 {
                    // Back to meta section
                    self.current_field = 1; // description
                } else {
                    // Back to previous color section
                    let section_idx = self.current_section - 1;
                    if let Some(section) = self.color_sections.get(section_idx) {
                        self.current_field = section.fields.len().saturating_sub(1);
                    }
                }
            } else {
                // Wrap around to last field of last section
                if !self.color_sections.is_empty() {
                    self.current_section = self.color_sections.len();
                    let section_idx = self.current_section - 1;
                    if let Some(section) = self.color_sections.get(section_idx) {
                        self.current_field = section.fields.len().saturating_sub(1);
                    }
                }
            }
        }
    }

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        _config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let width = 80;
        let height = 30;

        // Center popup
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(width)) / 2;
            self.popup_y = (area.height.saturating_sub(height)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;

        // Clear area
        for row in y..y + height {
            for col in x..x + width {
                if col < area.width && row < area.height {
                    buf[(col, row)]
                        .set_char(' ')
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.form_field_background));
                }
            }
        }

        // Draw border
        self.draw_border(x, y, width, height, buf, theme);

        // Title
        let title = " Theme Editor ";
        buf.set_string(
            x + 2,
            y,
            title,
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.form_label))
                .add_modifier(Modifier::BOLD),
        );

        // Instructions
        let footer = "Tab:Next  Shift+Tab:Prev  Ctrl+Enter:Save  Esc:Cancel";
        buf.set_string(
            x + 2,
            y + height - 1,
            footer,
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_disabled)),
        );

        // Render fields
        let mut current_y = y + 2;

        // Extract section and field to avoid borrow checker issues
        let current_section = self.current_section;
        let current_field = self.current_field;

        // Name field
        Self::render_text_field(
            "Name:",
            &mut self.name,
            x + 2,
            current_y,
            40,
            buf,
            theme,
            current_section == 0 && current_field == 0,
        );
        current_y += 2;

        // Description field
        Self::render_text_field(
            "Description:",
            &mut self.description,
            x + 2,
            current_y,
            40,
            buf,
            theme,
            current_section == 0 && current_field == 1,
        );
        current_y += 2;

        // Color section display
        if current_section > 0 && current_section <= self.color_sections.len() {
            let section_idx = current_section - 1;
            let section_name = self.color_sections[section_idx].name;

            // Section header
            buf.set_string(
                x + 2,
                current_y,
                format!("--- {} ---", section_name),
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.browser_title))
                    .add_modifier(Modifier::BOLD),
            );
            current_y += 1;

            // Show fields from current section (scroll if needed)
            let max_visible_fields = 10;
            let scroll_offset = if current_field >= max_visible_fields {
                current_field - max_visible_fields + 1
            } else {
                0
            };

            for (i, field) in self.color_sections[section_idx]
                .fields
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(max_visible_fields)
            {
                let is_focused = current_section > 0 && current_field == i;

                // Label
                let label_style = if is_focused {
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.form_label_focused))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label))
                };
                buf.set_string(
                    x + 4,
                    current_y,
                    format!("{:<20}", field.label),
                    label_style,
                );

                // Get value from textarea
                let value = field
                    .textarea
                    .lines().first()
                    .map(|s| s.as_str())
                    .unwrap_or("");

                // Value with focus indicator
                let value_style = if is_focused {
                    Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.form_field_text))
                        .bg(crossterm_bridge::to_ratatui_color(theme.form_field_background))
                } else {
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                };
                buf.set_string(x + 26, current_y, format!("{:<10}", value), value_style);

                // Color preview box (3 characters wide)
                if let Some(color) = ThemeData::parse_color(value) {
                    for offset in 0..3 {
                        buf[(x + 38 + offset, current_y)]
                            .set_char(' ')
                            .set_bg(crossterm_bridge::to_ratatui_color(color));
                    }
                }

                current_y += 1;
            }
        } else {
            // Show section list when in meta section
            current_y += 1;
            buf.set_string(
                x + 2,
                current_y,
                "Available Sections:",
                Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_secondary)),
            );
            current_y += 1;

            for (i, section) in self.color_sections.iter().enumerate().take(8) {
                buf.set_string(
                    x + 4,
                    current_y,
                    format!("{}. {}", i + 1, section.name),
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_primary)),
                );
                current_y += 1;
            }
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
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_border));

        // Top
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y)]
            .set_char('┐')
            .set_style(border_style);

        // Sides
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)]
                .set_char('│')
                .set_style(border_style);
        }

        // Bottom
        buf[(x, y + height - 1)]
            .set_char('└')
            .set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y + height - 1)]
                .set_char('─')
                .set_style(border_style);
        }
        buf[(x + width - 1, y + height - 1)]
            .set_char('┘')
            .set_style(border_style);
    }

    fn render_text_field(
        label: &str,
        textarea: &mut TextArea,
        x: u16,
        y: u16,
        width: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
        focused: bool,
    ) {
        let label_style = if focused {
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label_focused))
        } else {
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label))
        };

        buf.set_string(x, y, label, label_style);

        let input_area = Rect {
            x: x + 15,
            y,
            width,
            height: 1,
        };

        textarea.set_style(
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.form_field_text))
                .bg(crossterm_bridge::to_ratatui_color(theme.form_field_background)),
        );
        textarea.set_cursor_style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.editor_cursor)));
        RatatuiWidget::render(&*textarea, input_area, buf);
    }
}
