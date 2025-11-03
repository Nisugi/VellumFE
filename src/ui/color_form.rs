use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::TextArea;
use crate::config::PaletteColor;

/// Mode for the color form (Create new or Edit existing)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormMode {
    Create,
    Edit { original_name: [char; 64], original_len: usize },
}

/// Form for creating/editing color palette entries
pub struct ColorForm {
    // Form fields (TextArea)
    name: TextArea<'static>,
    color: TextArea<'static>,
    category: TextArea<'static>,
    favorite: bool,

    // UI state
    focused_field: usize, // 0=name, 1=color, 2=category, 3=favorite, 4=save, 5=delete, 6=cancel
    mode: FormMode,

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl ColorForm {
    /// Create a new empty form for adding a color
    pub fn new_create() -> Self {
        let mut name = TextArea::default();
        name.set_placeholder_text("e.g., Primary Blue");

        let mut color = TextArea::default();
        color.set_placeholder_text("e.g., #0066cc");

        let mut category = TextArea::default();
        category.set_placeholder_text("e.g., blues, reds, greens");

        Self {
            name,
            color,
            category,
            favorite: false,
            focused_field: 0,
            mode: FormMode::Create,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    /// Create a form for editing an existing color
    pub fn new_edit(palette_color: &PaletteColor) -> Self {
        let mut original_name = ['\0'; 64];
        let original_len = palette_color.name.len().min(64);
        for (i, ch) in palette_color.name.chars().take(64).enumerate() {
            original_name[i] = ch;
        }

        let mut name = TextArea::default();
        name.insert_str(&palette_color.name);

        let mut color = TextArea::default();
        color.insert_str(&palette_color.color);

        let mut category = TextArea::default();
        category.insert_str(&palette_color.category);

        Self {
            name,
            color,
            category,
            favorite: palette_color.favorite,
            focused_field: 0,
            mode: FormMode::Edit { original_name, original_len },
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn handle_input(&mut self, key_event: KeyEvent) -> Option<FormAction> {
        match key_event.code {
            KeyCode::Esc => {
                return Some(FormAction::Cancel);
            }
            KeyCode::Char('a') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+A to select all in current text field
                let textarea = match self.focused_field {
                    0 => &mut self.name,
                    1 => &mut self.color,
                    2 => &mut self.category,
                    _ => return None,
                };
                textarea.select_all();
                return None;
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                return self.validate_and_save();
            }
            KeyCode::BackTab => {
                self.previous_field();
                return None;
            }
            KeyCode::Tab => {
                self.next_field();
                return None;
            }
            KeyCode::Enter => {
                if self.focused_field == 3 {
                    // Toggle favorite on Enter when focused
                    self.favorite = !self.favorite;
                    return None;
                } else if self.focused_field >= 0 && self.focused_field <= 2 {
                    // In text field - move to next field
                    self.next_field();
                    return None;
                } else {
                    // On favorite or beyond - save the form
                    return self.validate_and_save();
                }
            }
            KeyCode::Char(' ') if self.focused_field == 3 => {
                // Space toggles favorite
                self.favorite = !self.favorite;
                return None;
            }
            KeyCode::Char('a') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+A to select all in current text field
                match self.focused_field {
                    0 => self.name.select_all(),
                    1 => self.category.select_all(),
                    2 => self.color.select_all(),
                    _ => {}
                }
                return None;
            }
            _ => {
                // Convert crossterm KeyEvent to ratatui KeyEvent for TextArea
                use ratatui::crossterm::event as rt_event;

                // Convert KeyCode
                let rt_code = match key_event.code {
                    KeyCode::Backspace => rt_event::KeyCode::Backspace,
                    KeyCode::Enter => rt_event::KeyCode::Enter,
                    KeyCode::Left => rt_event::KeyCode::Left,
                    KeyCode::Right => rt_event::KeyCode::Right,
                    KeyCode::Up => rt_event::KeyCode::Up,
                    KeyCode::Down => rt_event::KeyCode::Down,
                    KeyCode::Home => rt_event::KeyCode::Home,
                    KeyCode::End => rt_event::KeyCode::End,
                    KeyCode::PageUp => rt_event::KeyCode::PageUp,
                    KeyCode::PageDown => rt_event::KeyCode::PageDown,
                    KeyCode::Tab => rt_event::KeyCode::Tab,
                    KeyCode::BackTab => rt_event::KeyCode::BackTab,
                    KeyCode::Delete => rt_event::KeyCode::Delete,
                    KeyCode::Insert => rt_event::KeyCode::Insert,
                    KeyCode::F(n) => rt_event::KeyCode::F(n),
                    KeyCode::Char(c) => rt_event::KeyCode::Char(c),
                    KeyCode::Null => rt_event::KeyCode::Null,
                    KeyCode::Esc => rt_event::KeyCode::Esc,
                    _ => rt_event::KeyCode::Null,
                };

                // Convert KeyModifiers
                let mut rt_modifiers = rt_event::KeyModifiers::empty();
                if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                    rt_modifiers |= rt_event::KeyModifiers::SHIFT;
                }
                if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    rt_modifiers |= rt_event::KeyModifiers::CONTROL;
                }
                if key_event.modifiers.contains(KeyModifiers::ALT) {
                    rt_modifiers |= rt_event::KeyModifiers::ALT;
                }

                let rt_key = rt_event::KeyEvent {
                    code: rt_code,
                    modifiers: rt_modifiers,
                    kind: rt_event::KeyEventKind::Press,
                    state: rt_event::KeyEventState::empty(),
                };

                // Pass to the focused textarea
                match self.focused_field {
                    0 => { self.name.input(rt_key); }
                    1 => { self.category.input(rt_key); }
                    2 => { self.color.input(rt_key); }
                    _ => {}
                }
            }
        }

        None
    }

    fn next_field(&mut self) {
        self.focused_field = match self.focused_field { 0 => 1, 1 => 2, 2 => 3, _ => 0 };
    }

    fn previous_field(&mut self) {
        self.focused_field = match self.focused_field { 0 => 3, 1 => 0, 2 => 1, _ => 2 };
    }

    fn validate_and_save(&self) -> Option<FormAction> {
        let name_val = self.name.lines()[0].to_string();
        let color_val = self.color.lines()[0].to_string();
        let category_val = self.category.lines()[0].to_string();

        // Validate name
        if name_val.trim().is_empty() {
            return Some(FormAction::Error("Name cannot be empty".to_string()));
        }

        // Validate color (must be hex format)
        if !color_val.starts_with('#') || color_val.len() != 7 {
            return Some(FormAction::Error("Color must be in format #RRGGBB".to_string()));
        }

        // Validate hex digits
        if !color_val[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(FormAction::Error("Color must contain valid hex digits (0-9, A-F)".to_string()));
        }

        // Validate category
        if category_val.trim().is_empty() {
            return Some(FormAction::Error("Category cannot be empty".to_string()));
        }

        let original_name = if let FormMode::Edit { original_name, original_len } = self.mode {
            Some(original_name.iter().take(original_len).collect::<String>())
        } else {
            None
        };

        Some(FormAction::Save {
            color: PaletteColor {
                name: name_val.trim().to_string(),
                color: color_val.trim().to_uppercase(),
                category: category_val.trim().to_lowercase(),
                favorite: self.favorite,
            },
            original_name,
        })
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        let popup_width = 52;

        // Check if mouse is on title bar
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
                // Continue dragging
                self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
                self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);

                // Keep popup within bounds
                if self.popup_x + popup_width > area.width {
                    self.popup_x = area.width.saturating_sub(popup_width);
                }
                if self.popup_y + 9 > area.height {
                    self.popup_y = area.height.saturating_sub(9);
                }

                return true;
            } else {
                // Stop dragging
                self.is_dragging = false;
                return true;
            }
        }

        false
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        let popup_width = 52;
        let popup_height = 9;

        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(popup_height)) / 2;
        }

        // Parse textarea background color from config
        // If "-" is specified, use Color::Reset (terminal default), otherwise parse hex
        let textarea_bg = if config.colors.ui.textarea_background == "-" {
            Color::Reset
        } else if let Some(color) = Self::parse_hex_color(&config.colors.ui.textarea_background) {
            color
        } else {
            Color::Reset
        };

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width,
            height: popup_height,
        };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in self.popup_y..self.popup_y + popup_height {
            for col in self.popup_x..self.popup_x + popup_width {
                if col < area.width && row < area.height {
                    buf.set_string(col, row, " ", Style::default().bg(Color::Black));
                }
            }
        }

        // Draw border
        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        let top = format!("┌{}┐", "─".repeat(popup_width as usize - 2));
        buf.set_string(self.popup_x, self.popup_y, &top, border_style);

        // Title
        let title = match self.mode {
            FormMode::Create => " Add Color ",
            FormMode::Edit { .. } => " Edit Color ",
        };
        buf.set_string(self.popup_x + 2, self.popup_y, title, border_style.add_modifier(Modifier::BOLD));

        // Side borders
        for i in 1..popup_height - 1 {
            buf.set_string(self.popup_x, self.popup_y + i, "│", border_style);
            buf.set_string(self.popup_x + popup_width - 1, self.popup_y + i, "│", border_style);
        }

        // Bottom border
        let bottom = format!("└{}┘", "─".repeat(popup_width as usize - 2));
        buf.set_string(self.popup_x, self.popup_y + popup_height - 1, &bottom, border_style);

        // Render fields (single-line rows)
        let mut y = self.popup_y + 2;
        let focused = self.focused_field;

        // Name
        Self::render_text_field(focused, 0, "Name:", &mut self.name, self.popup_x + 2, y, popup_width, buf, textarea_bg);
        y += 1;

        // Category
        Self::render_text_field(focused, 1, "Category:", &mut self.category, self.popup_x + 2, y, popup_width, buf, textarea_bg);
        y += 1;

        // Color (10 chars) + preview
        let color_val = self.color.lines()[0].to_string();
        Self::render_color_field(focused, 2, "Color:", &mut self.color, &color_val, self.popup_x + 2, y, buf, textarea_bg);
        y += 1;

        // Favorite row
        Self::render_favorite_row(focused, 3, "Favorite:", self.favorite, self.popup_x + 2, y, popup_width, buf, textarea_bg);
        y += 2;

        // Status bar
        let status = "Tab:Next  Shift+Tab:Prev  Enter:Save  Esc:Close";
        buf.set_string(self.popup_x + 2, y, status, Style::default().fg(Color::Gray));
    }

    fn render_text_field(
        focused_field: usize,
        field_id: usize,
        label: &str,
        textarea: &mut TextArea,
        x: u16,
        y: u16,
        width: u16,
        buf: &mut Buffer,
        textarea_bg: Color,
    ) {
        // Label with focus color (yellow when focused, darker cyan otherwise)
        let is_focused = focused_field == field_id;
        let label_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Rgb(100, 149, 237))
        };
        let label_span = Span::styled(label, label_style);
        let label_area = Rect { x, y, width: 14, height: 1 };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        // Input style: background, cyan text; focused -> gold/yellow
        let base_style = Style::default().fg(Color::Cyan).bg(textarea_bg);
        let focused_style = Style::default().fg(Color::Black).bg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD);
        textarea.set_style(if focused_field == field_id { focused_style } else { base_style });
        textarea.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
        textarea.set_cursor_line_style(Style::default());

        // Set placeholder style to match (no underline)
        textarea.set_placeholder_style(Style::default().fg(Color::Gray).bg(textarea_bg));

        // Input area
        let input_area = Rect {
            x: x + 10,
            y,
            width: width.saturating_sub(14),
            height: 1,
        };

        // No borders on inputs
        textarea.set_block(Block::default().borders(Borders::NONE).style(base_style));

        RatatuiWidget::render(&*textarea, input_area, buf);
    }

    fn render_color_field(
        focused_field: usize,
        field_id: usize,
        label: &str,
        textarea: &mut TextArea,
        color_val: &str,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        textarea_bg: Color,
    ) {
        // Label with focus color (yellow when focused, darker cyan otherwise)
        let is_focused = focused_field == field_id;
        let label_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Rgb(100, 149, 237))
        };
        let label_span = Span::styled(label, label_style);
        let label_area = Rect { x, y, width: 14, height: 1 };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        // Styles
        let base_style = Style::default().fg(Color::Cyan).bg(textarea_bg);
        let focused_style = Style::default().fg(Color::Black).bg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD);
        textarea.set_style(if focused_field == field_id { focused_style } else { base_style });
        textarea.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(Style::default().fg(Color::Gray).bg(textarea_bg));

        // Fixed 10 char input
        let input_area = Rect { x: x + 10, y, width: 10, height: 1 };
        textarea.set_block(Block::default().borders(Borders::NONE).style(base_style));
        RatatuiWidget::render(&*textarea, input_area, buf);

        // Space then preview
        let preview_x = input_area.x + input_area.width + 1;
        if color_val.starts_with('#') && color_val.len() == 7 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&color_val[1..3], 16),
                u8::from_str_radix(&color_val[3..5], 16),
                u8::from_str_radix(&color_val[5..7], 16),
            ) {
                let style = Style::default().bg(Color::Rgb(r, g, b));
                buf.set_string(preview_x, y, "    ", style);
            }
        }
    }

    fn render_favorite_row(
        focused_field: usize,
        field_id: usize,
        label: &str,
        value: bool,
        x: u16,
        y: u16,
        _width: u16,
        buf: &mut Buffer,
        textarea_bg: Color,
    ) {
        // Label with focus color (yellow when focused, darker cyan otherwise)
        let is_focused = focused_field == field_id;
        let label_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Rgb(100, 149, 237))
        };
        let label_span = Span::styled(label, label_style);
        let label_area = Rect { x, y, width: 14, height: 1 };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        let base_style = Style::default().fg(Color::Cyan).bg(textarea_bg);
        let style = base_style;

        let val_text = if value { "[✓]" } else { "[ ]" };
        buf.set_string(x + 10, y, val_text, style);
    }

    fn render_color_preview(&self, color_str: &str, x: u16, y: u16, buf: &mut Buffer) {
        if !color_str.is_empty() && self.is_valid_hex_color(color_str) {
            if let Some(color) = self.parse_color(color_str) {
                let preview = "    ";
                buf.set_string(x, y, preview, Style::default().bg(color));
            }
        }
    }

    fn is_valid_hex_color(&self, color: &str) -> bool {
        if !color.starts_with('#') || color.len() != 7 {
            return false;
        }
        color[1..].chars().all(|c| c.is_ascii_hexdigit())
    }

    fn parse_color(&self, hex: &str) -> Option<Color> {
        if hex.len() != 7 || !hex.starts_with('#') {
            return None;
        }
        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    }

    /// Get the original name if in edit mode
    pub fn get_original_name(&self) -> Option<String> {
        match self.mode {
            FormMode::Edit { original_name, original_len } => {
                Some(original_name.iter().take(original_len).collect())
            }
            FormMode::Create => None,
        }
    }

    /// Parse hex color string to ratatui Color
    fn parse_hex_color(hex: &str) -> Option<Color> {
        if hex.starts_with('#') && hex.len() == 7 {
            let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
            let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
            let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum FormAction {
    Save { color: PaletteColor, original_name: Option<String> },
    Delete,
    Cancel,
    Error(String),
}
