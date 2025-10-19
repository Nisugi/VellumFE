use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget as RatatuiWidget, Block, Borders},
};
use tui_textarea::TextArea;
use crate::config::SpellColorRange;

#[derive(Debug, Clone, PartialEq)]
pub enum FormMode {
    Create,
    Edit(usize), // Index in spell_colors Vec
}

#[derive(Debug, Clone)]
pub enum SpellColorFormResult {
    Save(SpellColorRange),
    Delete(usize), // Index to delete
    Cancel,
}

pub struct SpellColorFormWidget {
    mode: FormMode,
    focused_field: usize,
    spell_ids: TextArea<'static>,
    bar_color: TextArea<'static>,
    text_color: TextArea<'static>,
    bg_color: TextArea<'static>,
    popup_position: (u16, u16), // (col, row)
    #[allow(unused)]
    pub is_dragging: bool,
    #[allow(unused)]
    drag_offset: (i16, i16),
}

impl SpellColorFormWidget {
    pub fn new() -> Self {
        let mut spell_ids = TextArea::default();
        spell_ids.set_placeholder_text("e.g., 905, 509, 1720");

        let mut bar_color = TextArea::default();
        bar_color.set_placeholder_text("e.g., #ff0000 or palette name");

        let mut text_color = TextArea::default();
        text_color.set_placeholder_text("e.g., #ffffff or name");
        text_color.insert_str("#ffffff");

        let mut bg_color = TextArea::default();
        bg_color.set_placeholder_text("e.g., #000000 or name");
        bg_color.insert_str("#000000");

        Self {
            mode: FormMode::Create,
            focused_field: 0,
            spell_ids,
            bar_color,
            text_color,
            bg_color,
            popup_position: (10, 2),
            is_dragging: false,
            drag_offset: (0, 0),
        }
    }

    pub fn new_edit(index: usize, spell_color: &SpellColorRange) -> Self {
        let spell_ids_str = spell_color.spells.iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let mut spell_ids = TextArea::default();
        spell_ids.insert_str(&spell_ids_str);

        let mut bar_color = TextArea::default();
        let bar_color_val = spell_color.bar_color.clone().unwrap_or_else(|| spell_color.color.clone());
        bar_color.insert_str(&bar_color_val);

        let mut text_color = TextArea::default();
        let text_color_val = spell_color.text_color.clone().unwrap_or_else(|| "#ffffff".to_string());
        text_color.insert_str(&text_color_val);

        let mut bg_color = TextArea::default();
        let bg_color_val = spell_color.bg_color.clone().unwrap_or_else(|| "#000000".to_string());
        bg_color.insert_str(&bg_color_val);

        Self {
            mode: FormMode::Edit(index),
            focused_field: 0,
            spell_ids,
            bar_color,
            text_color,
            bg_color,
            popup_position: (10, 2),
            is_dragging: false,
            drag_offset: (0, 0),
        }
    }

    pub fn input(&mut self, key: KeyEvent) -> Option<SpellColorFormResult> {
        match key.code {
            KeyCode::Esc => {
                return Some(SpellColorFormResult::Cancel);
            }
            KeyCode::Char('c') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return self.save();
                }
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
                // Move to next field
                self.next_field();
                return None;
            }
            _ => {
                // Convert crossterm KeyEvent to ratatui KeyEvent for TextArea
                use ratatui::crossterm::event as rt_event;

                // Convert KeyCode
                let rt_code = match key.code {
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
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    rt_modifiers |= rt_event::KeyModifiers::SHIFT;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    rt_modifiers |= rt_event::KeyModifiers::CONTROL;
                }
                if key.modifiers.contains(KeyModifiers::ALT) {
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
                    0 => { self.spell_ids.input(rt_key); }
                    1 => { self.bar_color.input(rt_key); }
                    2 => { self.text_color.input(rt_key); }
                    3 => { self.bg_color.input(rt_key); }
                    _ => {}
                }
            }
        }

        None
    }

    fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            0 => 1,  // spell_ids -> bar_color
            1 => 2,  // bar_color -> text_color
            2 => 3,  // text_color -> bg_color
            3 => 4,  // bg_color -> Save button
            4 => {
                if matches!(self.mode, FormMode::Edit(_)) {
                    5  // Save -> Delete button
                } else {
                    6  // Save -> Cancel button
                }
            }
            5 => 6,  // Delete -> Cancel button
            6 => 0,  // Cancel -> spell_ids
            _ => 0,
        };
    }

    fn previous_field(&mut self) {
        self.focused_field = match self.focused_field {
            0 => 6,  // spell_ids -> Cancel button
            1 => 0,  // bar_color -> spell_ids
            2 => 1,  // text_color -> bar_color
            3 => 2,  // bg_color -> text_color
            4 => 3,  // Save -> bg_color
            5 => 4,  // Delete -> Save
            6 => {
                if matches!(self.mode, FormMode::Edit(_)) {
                    5  // Cancel -> Delete button
                } else {
                    4  // Cancel -> Save button
                }
            }
            _ => 0,
        };
    }

    fn save(&self) -> Option<SpellColorFormResult> {
        // Get values from textareas
        let spell_ids_str = self.spell_ids.lines()[0].to_string();
        let bar_color_str = self.bar_color.lines()[0].to_string();
        let text_color_str = self.text_color.lines()[0].to_string();
        let bg_color_str = self.bg_color.lines()[0].to_string();

        // Validate and parse spell IDs
        let spell_ids: Vec<u32> = spell_ids_str
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect();

        if spell_ids.is_empty() {
            return None; // Invalid input
        }

        // Allow color names or hex; final resolution to hex occurs at app level
        // (App will resolve palette names to hex and validate before saving.)

        let spell_color = SpellColorRange {
            spells: spell_ids,
            color: bar_color_str.clone(), // Legacy field
            bar_color: if bar_color_str.is_empty() { None } else { Some(bar_color_str) },
            text_color: if text_color_str.is_empty() { None } else { Some(text_color_str) },
            bg_color: if bg_color_str.is_empty() { None } else { Some(bg_color_str) },
        };

        Some(SpellColorFormResult::Save(spell_color))
    }

    // Expose a safe save attempt for external handlers (e.g., app-level Ctrl+S interception)
    pub fn try_save(&self) -> Option<SpellColorFormResult> {
        self.save()
    }

    fn is_valid_hex_color(&self, color: &str) -> bool {
        if color.is_empty() {
            return true;
        }
        if !color.starts_with('#') || color.len() != 7 {
            return false;
        }
        color[1..].chars().all(|c| c.is_ascii_hexdigit())
    }

    #[allow(dead_code)]
    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> bool {
        let (col, row) = (event.column, event.row);
        let (popup_col, popup_row) = self.popup_position;
        let popup_width = 53;
        let popup_height = 9;

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if clicking in title bar (for dragging)
                if row == popup_row
                    && col > popup_col
                    && col < popup_col + popup_width - 1
                {
                    self.is_dragging = true;
                    self.drag_offset = (col as i16 - popup_col as i16, row as i16 - popup_row as i16);
                    return true;
                }

                // Check if clicking on buttons
                let button_row = popup_row + popup_height - 3;
                if row == button_row {
                    let save_col = popup_col + 2;
                    let delete_col = popup_col + 17;
                    let cancel_col = popup_col + 35;

                    if col >= save_col && col < save_col + 10 {
                        self.focused_field = 4;
                        return true;
                    } else if col >= delete_col && col < delete_col + 12 && matches!(self.mode, FormMode::Edit(_)) {
                        self.focused_field = 5;
                        return true;
                    } else if col >= cancel_col && col < cancel_col + 10 {
                        self.focused_field = 6;
                        return true;
                    }
                }

                // Check if clicking on text fields
                if col >= popup_col + 2 && col < popup_col + popup_width - 2 {
                    let field_row_spell_ids = popup_row + 2;
                    let field_row_bar_color = popup_row + 5;
                    let field_row_text_color = popup_row + 8;
                    let field_row_bg_color = popup_row + 11;

                    if row >= field_row_spell_ids && row < field_row_spell_ids + 3 {
                        self.focused_field = 0;
                        return true;
                    } else if row >= field_row_bar_color && row < field_row_bar_color + 3 {
                        self.focused_field = 1;
                        return true;
                    } else if row >= field_row_text_color && row < field_row_text_color + 3 {
                        self.focused_field = 2;
                        return true;
                    } else if row >= field_row_bg_color && row < field_row_bg_color + 3 {
                        self.focused_field = 3;
                        return true;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_dragging {
                    let new_col = (col as i16 - self.drag_offset.0).max(0) as u16;
                    let new_row = (row as i16 - self.drag_offset.1).max(0) as u16;
                    self.popup_position = (
                        new_col.min(area.width.saturating_sub(popup_width)),
                        new_row.min(area.height.saturating_sub(popup_height)),
                    );
                    return true;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_dragging {
                    self.is_dragging = false;
                    return true;
                }
            }
            _ => {}
        }

        false
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, config: &crate::config::Config) {
        let (popup_col, popup_row) = self.popup_position;
        let popup_width = 52;
        let popup_height = 9;

        // Parse textarea background color from config
        let textarea_bg = if let Some(color) = Self::parse_hex_color(&config.colors.ui.textarea_background) {
            color
        } else {
            Color::Rgb(53, 5, 5) // Fallback to maroon
        };

        // Draw black background
        for row in popup_row..popup_row + popup_height {
            for col in popup_col..popup_col + popup_width {
                if col < area.width && row < area.height {
                    buf.set_string(col, row, " ", Style::default().bg(Color::Black));
                }
            }
        }

        // Draw border
        let title = match self.mode {
            FormMode::Create => " Add Spell Color ",
            FormMode::Edit(_) => " Edit Spell Color ",
        };

        let border_style = Style::default().fg(Color::Cyan);

        // Top border
        let top = format!("┌{}┐", "─".repeat(popup_width as usize - 2));
        buf.set_string(popup_col, popup_row, &top, border_style);

        // Title
        buf.set_string(popup_col + 2, popup_row, title, border_style.add_modifier(Modifier::BOLD));

        // Side borders
        for i in 1..popup_height - 1 {
            buf.set_string(popup_col, popup_row + i, "│", border_style);
            buf.set_string(popup_col + popup_width - 1, popup_row + i, "│", border_style);
        }

        // Bottom border
        let bottom = format!("└{}┘", "─".repeat(popup_width as usize - 2));
        buf.set_string(popup_col, popup_row + popup_height - 1, &bottom, border_style);

        // Get color values before rendering (avoid multiple mutable borrows)
        let bar_color_val = self.bar_color.lines()[0].to_string();
        let text_color_val = self.text_color.lines()[0].to_string();
        let bg_color_val = self.bg_color.lines()[0].to_string();

        // Render fields
        let mut y = popup_row + 2;
        let focused = self.focused_field;

        // Spell IDs field (height 3)
        Self::render_text_field(focused, 0, "Spell IDs:", &mut self.spell_ids, popup_col + 2, y, popup_width - 4, buf, textarea_bg);
        y += 1;

        // Bar Color field (10 chars) + preview
        Self::render_color_field(focused, 1, "Bar Color:", &mut self.bar_color, &bar_color_val, popup_col + 2, y, buf, textarea_bg);
        y += 1;

        // Text Color field (10 chars) + preview
        Self::render_color_field(focused, 2, "Text Color:", &mut self.text_color, &text_color_val, popup_col + 2, y, buf, textarea_bg);
        y += 1;

        // Background Color field (10 chars) + preview
        Self::render_color_field(focused, 3, "Background:", &mut self.bg_color, &bg_color_val, popup_col + 2, y, buf, textarea_bg);
        y += 2;

        // Status bar
        let status = "Tab:Next  Shift+Tab:Prev  Ctrl+S:Save  Esc:Close";
        buf.set_string(popup_col + 2, y, status, Style::default().fg(Color::Gray));
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

        // Input style: cyan text on maroon; focused -> gold background
        let base_style = Style::default().fg(Color::Cyan).bg(textarea_bg);
        textarea.set_style(base_style);
        textarea.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
        textarea.set_cursor_line_style(Style::default());

        // Set placeholder style to match (no underline)
        textarea.set_placeholder_style(Style::default().fg(Color::Gray).bg(textarea_bg));

        // Input area - single line
        let input_area = Rect {
            x: x + 12,
            y,
            width: width.saturating_sub(12),
            height: 1,
        };

        // No border for inputs
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
        textarea.set_style(base_style);
        textarea.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(Style::default().fg(Color::Gray).bg(textarea_bg));

        // Fixed 10 char input
        let input_area = Rect { x: x + 12, y, width: 10, height: 1 };
        textarea.set_block(Block::default().borders(Borders::NONE).style(base_style));
        RatatuiWidget::render(&*textarea, input_area, buf);

        // Space then preview swatch
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

    fn render_color_preview(&self, color_str: &str, x: u16, y: u16, buf: &mut Buffer) {
        if !color_str.is_empty() && self.is_valid_hex_color(color_str) {
            if let Some(color) = self.parse_color(color_str) {
                // Draw a 4-character wide preview box
                let preview = "    ";
                buf.set_string(x, y, preview, Style::default().bg(color));
            }
        }
    }

    fn render_buttons(&self, x: u16, y: u16, buf: &mut Buffer) {
        let save_style = if self.focused_field == 4 {
            Style::default().fg(Color::Black).bg(Color::Green)
        } else {
            Style::default().fg(Color::Green)
        };
        buf.set_string(x, y, "[ Save ]", save_style);

        if matches!(self.mode, FormMode::Edit(_)) {
            let delete_style = if self.focused_field == 5 {
                Style::default().fg(Color::Black).bg(Color::Red)
            } else {
                Style::default().fg(Color::Red)
            };
            buf.set_string(x + 15, y, "[ Delete ]", delete_style);
        }

        let cancel_style = if self.focused_field == 6 {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(x + 33, y, "[ Cancel ]", cancel_style);
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

    /// Parse hex color string to ratatui Color (static version)
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
