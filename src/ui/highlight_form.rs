use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};
use tui_textarea::TextArea;
use regex::Regex;
use crate::config::HighlightPattern;

/// Form mode - Create new or Edit existing
#[derive(Debug, Clone, PartialEq)]
pub enum FormMode {
    Create,
    Edit(String),  // Contains original highlight name
}

/// Result of form submission
#[derive(Debug, Clone)]
pub enum FormResult {
    Save { name: String, pattern: HighlightPattern },
    Delete { name: String },
    Cancel,
}

/// Highlight management form widget
pub struct HighlightFormWidget {
    // Text input fields (using tui-textarea)
    name: TextArea<'static>,
    pattern: TextArea<'static>,
    fg_color: TextArea<'static>,
    bg_color: TextArea<'static>,
    sound: TextArea<'static>,
    sound_volume: TextArea<'static>,

    // Checkbox states
    bold: bool,
    color_entire_line: bool,
    fast_parse: bool,

    // Form state
    focused_field: usize,          // 0-11: which field has focus
    status_message: String,
    pattern_error: Option<String>,
    mode: FormMode,
}

impl HighlightFormWidget {
    /// Create a new highlight form (Create mode)
    pub fn new() -> Self {
        let mut name = TextArea::default();
        name.set_cursor_line_style(Style::default());
        name.set_placeholder_text("e.g., swing_highlight");

        let mut pattern = TextArea::default();
        pattern.set_cursor_line_style(Style::default());
        pattern.set_placeholder_text("e.g., You swing.*");

        let mut fg_color = TextArea::default();
        fg_color.set_cursor_line_style(Style::default());
        fg_color.set_placeholder_text("#ff0000");

        let mut bg_color = TextArea::default();
        bg_color.set_cursor_line_style(Style::default());
        bg_color.set_placeholder_text("(optional)");

        let mut sound = TextArea::default();
        sound.set_cursor_line_style(Style::default());
        sound.set_placeholder_text("sword_swing.wav");

        let mut sound_volume = TextArea::default();
        sound_volume.set_cursor_line_style(Style::default());
        sound_volume.set_placeholder_text("0.0-1.0 (e.g., 0.8)");

        Self {
            name,
            pattern,
            fg_color,
            bg_color,
            sound,
            sound_volume,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            focused_field: 0,
            status_message: "Ready".to_string(),
            pattern_error: None,
            mode: FormMode::Create,
        }
    }

    /// Create form in Edit mode with existing highlight
    pub fn new_edit(name: String, pattern: &HighlightPattern) -> Self {
        let mut form = Self::new();
        form.mode = FormMode::Edit(name.clone());

        // Load existing values
        form.name = TextArea::from([name.clone()]);
        form.name.set_cursor_line_style(Style::default());

        form.pattern = TextArea::from([pattern.pattern.clone()]);
        form.pattern.set_cursor_line_style(Style::default());

        if let Some(ref fg) = pattern.fg {
            form.fg_color = TextArea::from([fg.clone()]);
            form.fg_color.set_cursor_line_style(Style::default());
        }

        if let Some(ref bg) = pattern.bg {
            form.bg_color = TextArea::from([bg.clone()]);
            form.bg_color.set_cursor_line_style(Style::default());
        }

        if let Some(ref sound_file) = pattern.sound {
            form.sound = TextArea::from([sound_file.clone()]);
            form.sound.set_cursor_line_style(Style::default());
        }

        if let Some(volume) = pattern.sound_volume {
            form.sound_volume = TextArea::from([volume.to_string()]);
            form.sound_volume.set_cursor_line_style(Style::default());
        }

        form.bold = pattern.bold;
        form.color_entire_line = pattern.color_entire_line;
        form.fast_parse = pattern.fast_parse;

        form.status_message = "Editing highlight".to_string();
        form
    }

    /// Move focus to next field
    pub fn focus_next(&mut self) {
        self.focused_field = (self.focused_field + 1) % 12;
    }

    /// Move focus to previous field
    pub fn focus_prev(&mut self) {
        self.focused_field = if self.focused_field == 0 {
            11
        } else {
            self.focused_field - 1
        };
    }

    /// Handle key input for current focused field
    pub fn handle_key(&mut self, key: ratatui::crossterm::event::KeyEvent) -> Option<FormResult> {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.focus_prev();
                } else {
                    self.focus_next();
                }
                None
            }
            KeyCode::Esc => Some(FormResult::Cancel),
            KeyCode::Char(' ') if (4..=6).contains(&self.focused_field) => {
                // Toggle checkboxes
                match self.focused_field {
                    4 => self.bold = !self.bold,
                    5 => self.color_entire_line = !self.color_entire_line,
                    6 => self.fast_parse = !self.fast_parse,
                    _ => {}
                }
                None
            }
            KeyCode::Enter if self.focused_field == 9 => {
                // Save button
                self.try_save()
            }
            KeyCode::Enter if self.focused_field == 10 => {
                // Cancel button
                Some(FormResult::Cancel)
            }
            KeyCode::Enter if self.focused_field == 11 => {
                // Delete button (only in Edit mode)
                if let FormMode::Edit(ref name) = self.mode {
                    Some(FormResult::Delete { name: name.clone() })
                } else {
                    None
                }
            }
            _ => {
                // Pass key to appropriate text field
                // Convert KeyEvent to Input (tui-textarea expects Input)
                use tui_textarea::Input;
                let input: Input = key.into();

                let handled = match self.focused_field {
                    0 => self.name.input(input.clone()),
                    1 => {
                        let result = self.pattern.input(input.clone());
                        self.validate_pattern();
                        result
                    }
                    2 => self.fg_color.input(input.clone()),
                    3 => self.bg_color.input(input.clone()),
                    7 => self.sound.input(input.clone()),
                    8 => self.sound_volume.input(input.clone()),
                    _ => false,
                };

                // Log if not handled for debugging
                if !handled {
                    tracing::debug!("Key not handled by TextArea: {:?}", key);
                }

                None
            }
        }
    }

    /// Validate regex pattern
    fn validate_pattern(&mut self) {
        let pattern_text = self.pattern.lines()[0].as_str();
        if pattern_text.is_empty() {
            self.pattern_error = None;
            return;
        }

        match Regex::new(pattern_text) {
            Ok(_) => {
                self.pattern_error = None;
                self.status_message = "Pattern valid".to_string();
            }
            Err(e) => {
                self.pattern_error = Some(format!("Invalid regex: {}", e));
                self.status_message = "Invalid pattern!".to_string();
            }
        }
    }

    /// Try to save the form
    fn try_save(&self) -> Option<FormResult> {
        // Validate required fields
        let name = self.name.lines()[0].as_str().trim();
        if name.is_empty() {
            // Can't save without name
            return None;
        }

        let pattern_text = self.pattern.lines()[0].as_str().trim();
        if pattern_text.is_empty() {
            return None;
        }

        // Check pattern is valid
        if self.pattern_error.is_some() {
            return None;
        }

        // Build HighlightPattern
        let fg = {
            let fg_text = self.fg_color.lines()[0].as_str().trim();
            if fg_text.is_empty() {
                None
            } else {
                Some(fg_text.to_string())
            }
        };

        let bg = {
            let bg_text = self.bg_color.lines()[0].as_str().trim();
            if bg_text.is_empty() {
                None
            } else {
                Some(bg_text.to_string())
            }
        };

        let sound = {
            let sound_text = self.sound.lines()[0].as_str().trim();
            if sound_text.is_empty() {
                None
            } else {
                Some(sound_text.to_string())
            }
        };

        let sound_volume = {
            let vol_text = self.sound_volume.lines()[0].as_str().trim();
            if vol_text.is_empty() {
                None
            } else {
                vol_text.parse::<f32>().ok()
            }
        };

        let pattern = HighlightPattern {
            pattern: pattern_text.to_string(),
            fg,
            bg,
            bold: self.bold,
            color_entire_line: self.color_entire_line,
            fast_parse: self.fast_parse,
            sound,
            sound_volume,
        };

        Some(FormResult::Save {
            name: name.to_string(),
            pattern,
        })
    }

    /// Render the form as a centered popup
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Clear the area
        RatatuiWidget::render(Clear, area, buf);

        // Create centered popup - larger now with bordered text fields
        let popup_width = 62;
        let popup_height = 40;  // Increased from 28 to accommodate bordered fields

        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(popup_width)) / 2,
            y: area.y + (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width.min(area.width),
            height: popup_height.min(area.height),
        };

        // Render outer block
        let title = match &self.mode {
            FormMode::Create => "Add Highlight",
            FormMode::Edit(_) => "Edit Highlight",
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title)
            .title_alignment(Alignment::Center);

        RatatuiWidget::render(block, popup_area, buf);

        // Inner area for content
        let inner = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(2),
        };

        // Render fields
        self.render_fields(inner, buf);
    }

    /// Render all form fields
    fn render_fields(&mut self, area: Rect, buf: &mut Buffer) {
        let mut y = area.y;
        let focused = self.focused_field;

        // Name field (height 3)
        Self::render_text_field(0, focused, "Name:", &mut self.name, area.x, y, area.width, buf);
        y += 3;

        // Pattern field (height 3)
        Self::render_text_field(1, focused, "Pattern:", &mut self.pattern, area.x, y, area.width, buf);
        y += 3;
        if let Some(ref error) = self.pattern_error {
            let error_text = Paragraph::new(error.as_str())
                .style(Style::default().fg(Color::Red));
            let error_area = Rect { x: area.x + 12, y, width: area.width.saturating_sub(12), height: 1 };
            RatatuiWidget::render(error_text, error_area, buf);
            y += 1;
        }

        // Foreground color (height 3)
        let fg_line = self.fg_color.lines()[0].to_string();
        Self::render_text_field(2, focused, "Foreground:", &mut self.fg_color, area.x, y, area.width - 8, buf);
        // Color preview box (positioned in middle of the 3-row field)
        self.render_color_preview(&fg_line, area.x + area.width - 6, y + 1, buf);
        y += 3;

        // Background color (height 3)
        let bg_line = self.bg_color.lines()[0].to_string();
        Self::render_text_field(3, focused, "Background:", &mut self.bg_color, area.x, y, area.width - 8, buf);
        // Color preview box (positioned in middle of the 3-row field)
        self.render_color_preview(&bg_line, area.x + area.width - 6, y + 1, buf);
        y += 4;

        // Checkboxes (height 1 each)
        self.render_checkbox(4, "Bold", self.bold, area.x, y, buf);
        y += 1;
        self.render_checkbox(5, "Color entire line", self.color_entire_line, area.x, y, buf);
        y += 1;
        self.render_checkbox(6, "Fast parse", self.fast_parse, area.x, y, buf);
        y += 2;

        // Sound field (height 3)
        Self::render_text_field(7, focused, "Sound:", &mut self.sound, area.x, y, area.width, buf);
        y += 3;

        // Sound volume field (height 3) - value 0.0-1.0
        Self::render_text_field(8, focused, "Volume:", &mut self.sound_volume, area.x, y, 30, buf);
        y += 4;

        // Buttons
        self.render_buttons(area.x, y, buf);
        y += 2;

        // Status bar
        let status = Paragraph::new(self.status_message.as_str())
            .style(Style::default().fg(Color::Gray));
        let status_area = Rect { x: area.x, y, width: area.width, height: 1 };
        RatatuiWidget::render(status, status_area, buf);
    }

    /// Render a text input field
    fn render_text_field(
        field_id: usize,
        focused_field: usize,
        label: &str,
        textarea: &mut TextArea,
        x: u16,
        y: u16,
        width: u16,
        buf: &mut Buffer,
    ) {
        // Label
        let label_span = Span::styled(label, Style::default().fg(Color::White));
        let label_area = Rect { x, y, width: 12, height: 1 };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        // Set style based on focus
        let style = if focused_field == field_id {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        textarea.set_style(style);
        textarea.set_cursor_style(Style::default().bg(Color::Yellow));

        // Input area - TextArea needs height 3 minimum (border + text + cursor)
        let input_area = Rect {
            x: x + 12,
            y,
            width: width.saturating_sub(12),
            height: 3,
        };

        // Set border and style
        let border_style = if focused_field == field_id {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        textarea.set_block(Block::default().borders(Borders::ALL).border_style(border_style));

        // Render the TextArea
        // Note: Widget is implemented for &TextArea, not &mut TextArea
        RatatuiWidget::render(&*textarea, input_area, buf);
    }

    /// Render a checkbox
    fn render_checkbox(&self, field_id: usize, label: &str, checked: bool, x: u16, y: u16, buf: &mut Buffer) {
        let style = if self.focused_field == field_id {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let checkbox_text = if checked { "[X] " } else { "[ ] " };
        let text = format!("{}{}", checkbox_text, label);

        let para = Paragraph::new(text).style(style);
        let area = Rect { x, y, width: 30, height: 1 };
        RatatuiWidget::render(para, area, buf);
    }

    /// Render color preview box
    fn render_color_preview(&self, color_text: &str, x: u16, y: u16, buf: &mut Buffer) {
        let color_text = color_text.trim();

        if color_text.is_empty() {
            // Empty box
            let para = Paragraph::new("[    ]").style(Style::default().fg(Color::DarkGray));
            let area = Rect { x, y, width: 6, height: 1 };
            RatatuiWidget::render(para, area, buf);
            return;
        }

        // Try to parse hex color
        if let Ok(color) = Self::parse_hex_color(color_text) {
            let block = Block::default().style(Style::default().bg(color));
            let area = Rect { x, y, width: 4, height: 1 };
            RatatuiWidget::render(block, area, buf);
        } else {
            // Invalid color
            let para = Paragraph::new("[ERR]").style(Style::default().fg(Color::Red));
            let area = Rect { x, y, width: 6, height: 1 };
            RatatuiWidget::render(para, area, buf);
        }
    }

    /// Parse hex color string (#RRGGBB)
    fn parse_hex_color(hex: &str) -> Result<Color, ()> {
        if !hex.starts_with('#') || hex.len() != 7 {
            return Err(());
        }

        let r = u8::from_str_radix(&hex[1..3], 16).map_err(|_| ())?;
        let g = u8::from_str_radix(&hex[3..5], 16).map_err(|_| ())?;
        let b = u8::from_str_radix(&hex[5..7], 16).map_err(|_| ())?;

        Ok(Color::Rgb(r, g, b))
    }

    /// Render action buttons
    fn render_buttons(&self, x: u16, y: u16, buf: &mut Buffer) {
        // Save button
        let (save_text, save_style) = if self.focused_field == 9 {
            // Focused: inverted colors with bold
            ("[ SAVE ]", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD))
        } else {
            // Not focused: just green text
            ("[ Save ]", Style::default().fg(Color::Green))
        };
        let save_para = Paragraph::new(save_text).style(save_style);
        let save_area = Rect { x, y, width: 9, height: 1 };
        RatatuiWidget::render(save_para, save_area, buf);

        // Cancel button
        let (cancel_text, cancel_style) = if self.focused_field == 10 {
            // Focused: inverted colors with bold
            ("[ CANCEL ]", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD))
        } else {
            // Not focused: just red text
            ("[ Cancel ]", Style::default().fg(Color::Red))
        };
        let cancel_para = Paragraph::new(cancel_text).style(cancel_style);
        let cancel_area = Rect { x: x + 11, y, width: 11, height: 1 };
        RatatuiWidget::render(cancel_para, cancel_area, buf);

        // Delete button (only in Edit mode)
        if matches!(self.mode, FormMode::Edit(_)) {
            let (delete_text, delete_style) = if self.focused_field == 11 {
                // Focused: inverted colors with bold
                ("[ DELETE ]", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
            } else {
                // Not focused: just yellow text
                ("[ Delete ]", Style::default().fg(Color::Yellow))
            };
            let delete_para = Paragraph::new(delete_text).style(delete_style);
            let delete_area = Rect { x: x + 24, y, width: 11, height: 1 };
            RatatuiWidget::render(delete_para, delete_area, buf);
        }
    }
}
