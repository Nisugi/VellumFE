use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use crossterm::event::{KeyCode, KeyModifiers};
use crate::config::PaletteColor;

/// Mode for the color form (Create new or Edit existing)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormMode {
    Create,
    Edit { original_name: [char; 64], original_len: usize },
}

/// Form for creating/editing color palette entries
pub struct ColorForm {
    // Form fields
    name: String,
    color: String,
    category: String,
    favorite: bool,

    // UI state
    focused_field: usize, // 0=name, 1=color, 2=category, 3=favorite, 4=save, 5=cancel
    mode: FormMode,
    cursor_pos: usize,

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
        Self {
            name: String::new(),
            color: String::new(),
            category: String::new(),
            favorite: false,
            focused_field: 0,
            mode: FormMode::Create,
            cursor_pos: 0,
            popup_x: 20,
            popup_y: 5,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    /// Create a form for editing an existing color
    pub fn new_edit(color: &PaletteColor) -> Self {
        let mut original_name = ['\0'; 64];
        let original_len = color.name.len().min(64);
        for (i, ch) in color.name.chars().take(64).enumerate() {
            original_name[i] = ch;
        }

        Self {
            name: color.name.clone(),
            color: color.color.clone(),
            category: color.category.clone(),
            favorite: color.favorite,
            focused_field: 0,
            mode: FormMode::Edit { original_name, original_len },
            cursor_pos: 0,
            popup_x: 20,
            popup_y: 5,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    pub fn handle_input(&mut self, key: KeyCode, _modifiers: KeyModifiers) -> Option<FormAction> {
        match key {
            KeyCode::Esc => {
                return Some(FormAction::Cancel);
            }
            KeyCode::Tab => {
                // Move to next field
                self.focused_field = (self.focused_field + 1) % 6;
                self.update_cursor_pos();
            }
            KeyCode::BackTab => {
                // Move to previous field (Shift+Tab)
                if self.focused_field == 0 {
                    self.focused_field = 5;
                } else {
                    self.focused_field -= 1;
                }
                self.update_cursor_pos();
            }
            KeyCode::Enter => {
                if self.focused_field == 4 {
                    // Save button
                    return self.validate_and_save();
                } else if self.focused_field == 5 {
                    // Cancel button
                    return Some(FormAction::Cancel);
                } else if self.focused_field == 3 {
                    // Toggle favorite checkbox
                    self.favorite = !self.favorite;
                }
            }
            KeyCode::Char(c) => {
                match self.focused_field {
                    0 => {
                        // Name field
                        self.name.push(c);
                        self.cursor_pos += 1;
                    }
                    1 => {
                        // Color field - only allow valid hex characters
                        if c == '#' || c.is_ascii_hexdigit() {
                            self.color.push(c);
                            self.cursor_pos += 1;
                        }
                    }
                    2 => {
                        // Category field
                        self.category.push(c);
                        self.cursor_pos += 1;
                    }
                    3 => {
                        // Favorite - space or x toggles
                        if c == ' ' || c == 'x' || c == 'X' {
                            self.favorite = !self.favorite;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                match self.focused_field {
                    0 => {
                        if !self.name.is_empty() && self.cursor_pos > 0 {
                            self.name.pop();
                            self.cursor_pos -= 1;
                        }
                    }
                    1 => {
                        if !self.color.is_empty() && self.cursor_pos > 0 {
                            self.color.pop();
                            self.cursor_pos -= 1;
                        }
                    }
                    2 => {
                        if !self.category.is_empty() && self.cursor_pos > 0 {
                            self.category.pop();
                            self.cursor_pos -= 1;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                let max_pos = match self.focused_field {
                    0 => self.name.len(),
                    1 => self.color.len(),
                    2 => self.category.len(),
                    _ => 0,
                };
                if self.cursor_pos < max_pos {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = match self.focused_field {
                    0 => self.name.len(),
                    1 => self.color.len(),
                    2 => self.category.len(),
                    _ => 0,
                };
            }
            _ => {}
        }

        None
    }

    fn update_cursor_pos(&mut self) {
        self.cursor_pos = match self.focused_field {
            0 => self.name.len(),
            1 => self.color.len(),
            2 => self.category.len(),
            _ => 0,
        };
    }

    fn validate_and_save(&self) -> Option<FormAction> {
        // Validate name
        if self.name.trim().is_empty() {
            return Some(FormAction::Error("Name cannot be empty".to_string()));
        }

        // Validate color (must be hex format)
        if !self.color.starts_with('#') || self.color.len() != 7 {
            return Some(FormAction::Error("Color must be in format #RRGGBB".to_string()));
        }

        // Validate hex digits
        if !self.color[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(FormAction::Error("Color must contain valid hex digits (0-9, A-F)".to_string()));
        }

        // Validate category
        if self.category.trim().is_empty() {
            return Some(FormAction::Error("Category cannot be empty".to_string()));
        }

        let original_name = if let FormMode::Edit { original_name, original_len } = self.mode {
            Some(original_name.iter().take(original_len).collect::<String>())
        } else {
            None
        };

        Some(FormAction::Save {
            color: PaletteColor {
                name: self.name.trim().to_string(),
                color: self.color.trim().to_uppercase(),
                category: self.category.trim().to_lowercase(),
                favorite: self.favorite,
            },
            original_name,
        })
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        let popup_width = 60;

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
                if self.popup_y + 15 > area.height {
                    self.popup_y = area.height.saturating_sub(15);
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

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_width = 60;
        let popup_height = 15;

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width.min(area.width.saturating_sub(self.popup_x)),
            height: popup_height.min(area.height.saturating_sub(self.popup_y)),
        };

        // Draw solid black background
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ');
                    cell.set_bg(Color::Black);
                }
            }
        }

        // Draw border
        let border_style = Style::default().fg(Color::Cyan);
        self.draw_border(popup_area, buf, border_style);

        // Draw title
        let title = match self.mode {
            FormMode::Create => " Add Color ",
            FormMode::Edit { .. } => " Edit Color ",
        };
        let title_x = popup_area.x + 2;
        for (i, ch) in title.chars().enumerate() {
            let x = title_x + i as u16;
            if x >= popup_area.x + popup_area.width {
                break;
            }
            if let Some(cell) = buf.cell_mut((x, popup_area.y)) {
                cell.set_char(ch);
                cell.set_fg(Color::Yellow);
                cell.set_bg(Color::Black);
            }
        }

        // Draw form fields
        let form_x = popup_area.x + 2;
        let mut y = popup_area.y + 2;

        // Name field
        self.render_field("Name:", &self.name, form_x, y, popup_area.width - 4, buf, self.focused_field == 0);
        y += 2;

        // Color field with preview
        self.render_field("Color:", &self.color, form_x, y, popup_area.width - 4, buf, self.focused_field == 1);
        // Draw color preview
        if let Some(preview_color) = Self::parse_hex_color(&self.color) {
            let preview_x = form_x + popup_area.width.saturating_sub(10);
            for i in 0..3 {
                if let Some(cell) = buf.cell_mut((preview_x + i, y)) {
                    cell.set_char('█');
                    cell.set_fg(preview_color);
                    cell.set_bg(Color::Black);
                }
            }
        }
        y += 2;

        // Category field
        self.render_field("Category:", &self.category, form_x, y, popup_area.width - 4, buf, self.focused_field == 2);
        y += 2;

        // Favorite checkbox
        let checkbox = if self.favorite { "[X]" } else { "[ ]" };
        let checkbox_style = if self.focused_field == 3 {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::White).bg(Color::Black)
        };

        let label = "Favorite: ";
        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((form_x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_fg(Color::White);
                cell.set_bg(Color::Black);
            }
        }
        for (i, ch) in checkbox.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((form_x + label.len() as u16 + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(checkbox_style);
            }
        }
        y += 2;

        // Buttons
        let save_style = if self.focused_field == 4 {
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green).bg(Color::Black)
        };
        let cancel_style = if self.focused_field == 5 {
            Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red).bg(Color::Black)
        };

        let save_btn = " Save ";
        let cancel_btn = " Cancel ";
        let btn_x = form_x + 10;

        for (i, ch) in save_btn.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((btn_x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(save_style);
            }
        }
        for (i, ch) in cancel_btn.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((btn_x + save_btn.len() as u16 + 3 + i as u16, y)) {
                cell.set_char(ch);
                cell.set_style(cancel_style);
            }
        }

        // Draw help text
        let help = " Tab:Next Field  Shift+Tab:Prev  Enter:Save  Esc:Cancel ";
        let help_x = popup_area.x + popup_area.width.saturating_sub(help.len() as u16 + 1);
        if help_x > popup_area.x {
            for (i, ch) in help.chars().enumerate() {
                let x = help_x + i as u16;
                if x >= popup_area.x + popup_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, popup_area.y + popup_area.height - 1)) {
                    cell.set_char(ch);
                    cell.set_fg(Color::Gray);
                    cell.set_bg(Color::Black);
                }
            }
        }
    }

    fn render_field(&self, label: &str, value: &str, x: u16, y: u16, width: u16, buf: &mut Buffer, is_focused: bool) {
        // Draw label
        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = buf.cell_mut((x + i as u16, y)) {
                cell.set_char(ch);
                cell.set_fg(Color::White);
                cell.set_bg(Color::Black);
            }
        }

        // Draw input box
        let input_x = x + label.len() as u16 + 1;
        let input_width = width.saturating_sub(label.len() as u16 + 1);

        let style = if is_focused {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        };

        for i in 0..input_width {
            if let Some(cell) = buf.cell_mut((input_x + i, y)) {
                if i < value.len() as u16 {
                    cell.set_char(value.chars().nth(i as usize).unwrap_or(' '));
                } else {
                    cell.set_char(' ');
                }
                cell.set_style(style);
            }
        }
    }

    fn draw_border(&self, area: Rect, buf: &mut Buffer, style: Style) {
        // Top border
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                if x == area.x {
                    cell.set_char('╔');
                } else if x == area.x + area.width - 1 {
                    cell.set_char('╗');
                } else {
                    cell.set_char('═');
                }
                cell.set_style(style);
            }
        }

        // Bottom border
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y + area.height - 1)) {
                if x == area.x {
                    cell.set_char('╚');
                } else if x == area.x + area.width - 1 {
                    cell.set_char('╝');
                } else {
                    cell.set_char('═');
                }
                cell.set_style(style);
            }
        }

        // Left and right borders
        for y in area.y + 1..area.y + area.height - 1 {
            if let Some(cell) = buf.cell_mut((area.x, y)) {
                cell.set_char('║');
                cell.set_style(style);
            }
            if let Some(cell) = buf.cell_mut((area.x + area.width - 1, y)) {
                cell.set_char('║');
                cell.set_style(style);
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
}

#[derive(Debug, Clone)]
pub enum FormAction {
    Save { color: PaletteColor, original_name: Option<String> },
    Cancel,
    Error(String),
}
