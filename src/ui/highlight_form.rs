use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget},
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
    category: TextArea<'static>,
    fg_color: TextArea<'static>,
    bg_color: TextArea<'static>,
    sound: TextArea<'static>,
    sound_volume: TextArea<'static>,

    // Checkbox states
    bold: bool,
    color_entire_line: bool,
    fast_parse: bool,

    // Form state
    focused_field: usize,          // 0-9: which field has focus (0-6 text, 7-9 checkboxes)
    status_message: String,
    pattern_error: Option<String>,
    mode: FormMode,

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
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

        let mut category = TextArea::default();
        category.set_cursor_line_style(Style::default());
        category.set_placeholder_text("e.g., Combat, Loot, Spells");

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
            category,
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
            popup_x: 10,
            popup_y: 2,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
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

        if let Some(ref cat) = pattern.category {
            form.category = TextArea::from([cat.clone()]);
            form.category.set_cursor_line_style(Style::default());
        }

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

    /// Alias for new_edit - create form in Edit mode with existing highlight
    pub fn with_pattern(name: String, pattern: HighlightPattern) -> Self {
        Self::new_edit(name, &pattern)
    }

    /// Move focus to next field
    pub fn focus_next(&mut self) {
        self.focused_field = (self.focused_field + 1) % 10;
    }

    /// Move focus to previous field
    pub fn focus_prev(&mut self) {
        self.focused_field = if self.focused_field == 0 {
            9
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
            KeyCode::Up => {
                self.focus_prev();
                None
            }
            KeyCode::Down => {
                self.focus_next();
                None
            }
            KeyCode::Esc => Some(FormResult::Cancel),
            KeyCode::Char('s') | KeyCode::Char('S') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+S to save
                self.try_save()
            }
            KeyCode::Char(' ') | KeyCode::Enter if (7..=9).contains(&self.focused_field) => {
                // Toggle checkboxes (fields 7-9)
                match self.focused_field {
                    7 => self.bold = !self.bold,
                    8 => self.color_entire_line = !self.color_entire_line,
                    9 => self.fast_parse = !self.fast_parse,
                    _ => {}
                }
                None
            }
            KeyCode::Delete if matches!(self.mode, FormMode::Edit(_)) => {
                // Delete key in edit mode
                if let FormMode::Edit(ref name) = self.mode {
                    Some(FormResult::Delete { name: name.clone() })
                } else {
                    None
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(self.mode, FormMode::Edit(_)) => {
                // Ctrl+D to delete in edit mode
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
                    2 => self.category.input(input.clone()),
                    3 => self.fg_color.input(input.clone()),
                    4 => self.bg_color.input(input.clone()),
                    5 => self.sound.input(input.clone()),
                    6 => self.sound_volume.input(input.clone()),
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

        let category = {
            let cat_text = self.category.lines()[0].as_str().trim();
            if cat_text.is_empty() {
                None
            } else {
                Some(cat_text.to_string())
            }
        };

        let pattern = HighlightPattern {
            pattern: pattern_text.to_string(),
            category,
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

    /// Render the form as a draggable popup
    pub fn render(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        let width = 62;
        let height = 20; // Reduced from 40 to fit style guide pattern

        // Center popup initially
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(width)) / 2;
            self.popup_y = (area.height.saturating_sub(height)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;

        // Draw black background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    buf[(x + col, y + row)].set_char(' ').set_bg(Color::Black);
                }
            }
        }

        // Draw cyan border
        self.draw_border(x, y, width, height, buf);

        // Title (left-aligned)
        let title = match &self.mode {
            FormMode::Create => " Add Highlight ",
            FormMode::Edit(_) => " Edit Highlight ",
        };
        for (i, ch) in title.chars().enumerate() {
            if (x + 1 + i as u16) < (x + width) {
                buf[(x + 1 + i as u16, y)].set_char(ch).set_fg(Color::Cyan).set_bg(Color::Black);
            }
        }

        // Render fields
        self.render_fields(x, y, width, height, buf, config);

        // Footer
        let footer = " Ctrl+S:Save | Del/Ctrl+D:Delete | Esc:Cancel ";
        let footer_y = y + height - 2;
        let footer_x = x + ((width - footer.len() as u16) / 2);
        for (i, ch) in footer.chars().enumerate() {
            buf[(footer_x + i as u16, footer_y)].set_char(ch).set_fg(Color::White).set_bg(Color::Black);
        }
    }

    fn draw_border(&self, x: u16, y: u16, width: u16, height: u16, buf: &mut Buffer) {
        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y)].set_char('┐').set_style(border_style);

        // Side borders
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)].set_char('│').set_style(border_style);
        }

        // Bottom border
        buf[(x, y + height - 1)].set_char('└').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y + height - 1)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y + height - 1)].set_char('┘').set_style(border_style);
    }

    /// Render all form fields
    fn render_fields(&mut self, x: u16, y: u16, width: u16, height: u16, buf: &mut Buffer, config: &crate::config::Config) {
        let mut current_y = y + 2; // Start below title bar
        let label_width = 16; // Enough for "Background:"
        let input_start = x + 2 + label_width;
        let maroon = Color::Rgb(64, 0, 0);

        // Field 0: Name
        self.render_text_row(0, "Name:", &self.name, x + 2, current_y, input_start, 30, maroon, buf);
        current_y += 1;

        // Field 1: Pattern
        self.render_text_row(1, "Pattern:", &self.pattern, x + 2, current_y, input_start, 30, maroon, buf);
        current_y += 1;

        // Field 2: Category
        self.render_text_row(2, "Category:", &self.category, x + 2, current_y, input_start, 30, maroon, buf);
        current_y += 1;

        // Field 3: Foreground (10 char + 1 space + 2 char preview)
        self.render_color_row(3, "Foreground:", &self.fg_color, x + 2, current_y, input_start, maroon, buf, config);
        current_y += 1;

        // Field 4: Background (10 char + 1 space + 2 char preview)
        self.render_color_row(4, "Background:", &self.bg_color, x + 2, current_y, input_start, maroon, buf, config);
        current_y += 1;

        // Field 5: Sound
        self.render_text_row(5, "Sound:", &self.sound, x + 2, current_y, input_start, 30, maroon, buf);
        current_y += 1;

        // Field 6: Volume
        self.render_text_row(6, "Volume:", &self.sound_volume, x + 2, current_y, input_start, 10, maroon, buf);
        current_y += 2;

        // Checkboxes (Fields 7-9)
        self.render_checkbox(7, "Bold", self.bold, x + 2, current_y);
        buf[(x + 2, current_y)].set_char('[').set_fg(if self.focused_field == 7 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        buf[(x + 3, current_y)].set_char(if self.bold { '✓' } else { ' ' }).set_fg(if self.focused_field == 7 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        buf[(x + 4, current_y)].set_char(']').set_fg(if self.focused_field == 7 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        let bold_label = " Bold";
        for (i, ch) in bold_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)].set_char(ch).set_fg(if self.focused_field == 7 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        }
        current_y += 1;

        self.render_checkbox(8, "Color entire line", self.color_entire_line, x + 2, current_y);
        buf[(x + 2, current_y)].set_char('[').set_fg(if self.focused_field == 8 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        buf[(x + 3, current_y)].set_char(if self.color_entire_line { '✓' } else { ' ' }).set_fg(if self.focused_field == 8 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        buf[(x + 4, current_y)].set_char(']').set_fg(if self.focused_field == 8 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        let cel_label = " Color entire line";
        for (i, ch) in cel_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)].set_char(ch).set_fg(if self.focused_field == 8 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        }
        current_y += 1;

        self.render_checkbox(9, "Fast parse", self.fast_parse, x + 2, current_y);
        buf[(x + 2, current_y)].set_char('[').set_fg(if self.focused_field == 9 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        buf[(x + 3, current_y)].set_char(if self.fast_parse { '✓' } else { ' ' }).set_fg(if self.focused_field == 9 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        buf[(x + 4, current_y)].set_char(']').set_fg(if self.focused_field == 9 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        let fp_label = " Fast parse";
        for (i, ch) in fp_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)].set_char(ch).set_fg(if self.focused_field == 9 { Color::Rgb(255, 215, 0) } else { Color::Cyan }).set_bg(Color::Black);
        }
    }

    fn render_text_row(&self, field_id: usize, label: &str, textarea: &TextArea, x: u16, y: u16, input_x: u16, input_width: u16, bg: Color, buf: &mut Buffer) {
        let focused = self.focused_field == field_id;
        let label_color = if focused { Color::Rgb(255, 215, 0) } else { Color::Cyan };

        // Render label
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)].set_char(ch).set_fg(label_color).set_bg(Color::Black);
        }

        // Render input background
        for i in 0..input_width {
            buf[(input_x + i, y)].set_bg(bg);
        }

        // Render text content
        let text = &textarea.lines()[0];
        for (i, ch) in text.chars().enumerate().take(input_width as usize) {
            buf[(input_x + i as u16, y)].set_char(ch).set_fg(Color::White).set_bg(bg);
        }
    }

    fn render_color_row(&self, field_id: usize, label: &str, textarea: &TextArea, x: u16, y: u16, input_x: u16, bg: Color, buf: &mut Buffer, config: &crate::config::Config) {
        let focused = self.focused_field == field_id;
        let label_color = if focused { Color::Rgb(255, 215, 0) } else { Color::Cyan };

        // Render label
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)].set_char(ch).set_fg(label_color).set_bg(Color::Black);
        }

        // Render 10-char input background
        for i in 0..10 {
            buf[(input_x + i, y)].set_bg(bg);
        }

        // Render text content (max 10 chars)
        let text = &textarea.lines()[0];
        for (i, ch) in text.chars().enumerate().take(10) {
            buf[(input_x + i as u16, y)].set_char(ch).set_fg(Color::White).set_bg(bg);
        }

        // 1 space gap
        buf[(input_x + 10, y)].set_char(' ').set_bg(Color::Black);

        // 2-char color preview (no brackets)
        if let Some(color) = self.parse_and_resolve_color(text, config) {
            buf[(input_x + 11, y)].set_char(' ').set_bg(color);
            buf[(input_x + 12, y)].set_char(' ').set_bg(color);
        }
    }

    fn parse_and_resolve_color(&self, color_text: &str, config: &crate::config::Config) -> Option<Color> {
        let trimmed = color_text.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Try resolving through config
        if let Some(hex) = config.resolve_color(trimmed) {
            return Self::parse_hex_color(&hex).ok();
        }

        // Try parsing directly as hex
        Self::parse_hex_color(trimmed).ok()
    }

    fn render_checkbox(&self, field_id: usize, label: &str, checked: bool, x: u16, y: u16) {
        // No-op stub - checkboxes are rendered inline in render_fields
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

    /// Handle mouse events for dragging
    pub fn handle_mouse(&mut self, col: u16, row: u16, pressed: bool, terminal_area: Rect) -> bool {
        let popup_width = 62;
        let popup_height = 20;

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width.min(terminal_area.width.saturating_sub(self.popup_x)),
            height: popup_height.min(terminal_area.height.saturating_sub(self.popup_y)),
        };

        // Check if click is on title bar (top border, excluding corners)
        let on_title_bar = row == popup_area.y
            && col > popup_area.x
            && col < popup_area.x + popup_area.width - 1;

        if pressed {
            if on_title_bar && !self.is_dragging {
                // Start dragging
                self.is_dragging = true;
                self.drag_offset_x = col.saturating_sub(self.popup_x);
                self.drag_offset_y = row.saturating_sub(self.popup_y);
                return true;
            } else if self.is_dragging {
                // Continue dragging
                let new_x = col.saturating_sub(self.drag_offset_x);
                let new_y = row.saturating_sub(self.drag_offset_y);

                // Clamp to terminal bounds
                self.popup_x = new_x.min(terminal_area.width.saturating_sub(popup_width));
                self.popup_y = new_y.min(terminal_area.height.saturating_sub(popup_height));
                return true;
            }
        } else {
            // Mouse released
            if self.is_dragging {
                self.is_dragging = false;
                return true;
            }
        }

        false
    }
}
