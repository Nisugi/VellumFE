use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::TextArea;

use crate::config::PaletteColor;
use crate::ui::popup::{self, PopupState};
use crate::ui::theme::{self, TextInputStyle};

const POPUP_WIDTH: u16 = 52;
const POPUP_HEIGHT: u16 = 9;

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

    // Popup
    popup: PopupState,
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
            popup: PopupState::new(20, 5),
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
            popup: PopupState::new(20, 5),
        }
    }

    pub fn handle_input(&mut self, key_event: KeyEvent) -> Option<FormAction> {
        match key_event.code {
            KeyCode::Esc => return Some(FormAction::Cancel),
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
                    self.favorite = !self.favorite;
                    return None;
                }
                self.next_field();
                return None;
            }
            KeyCode::Char(' ') if self.focused_field == 3 => {
                self.favorite = !self.favorite;
                return None;
            }
            _ => {}
        }

        // Convert crossterm event to ratatui event for text areas
        use ratatui::crossterm::event as rt_event;

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

        match self.focused_field {
            0 => {
                self.name.input(rt_key);
            }
            1 => {
                self.category.input(rt_key);
            }
            2 => {
                self.color.input(rt_key);
            }
            _ => {}
        };

        None
    }

    fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            0 => 1,
            1 => 2,
            2 => 3,
            _ => 0,
        };
    }

    fn previous_field(&mut self) {
        self.focused_field = match self.focused_field {
            0 => 3,
            1 => 0,
            2 => 1,
            _ => 2,
        };
    }

    fn validate_and_save(&self) -> Option<FormAction> {
        let name_val = self.name.lines()[0].to_string();
        let color_val = self.color.lines()[0].to_string();
        let category_val = self.category.lines()[0].to_string();

        if name_val.trim().is_empty() {
            return Some(FormAction::Error("Name cannot be empty".to_string()));
        }

        let color_trimmed = color_val.trim();

        if !Self::is_valid_hex_color(color_trimmed) {
            return Some(FormAction::Error("Color must be in format #RRGGBB".to_string()));
        }

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
                color: color_trimmed.to_uppercase(),
                category: category_val.trim().to_lowercase(),
                favorite: self.favorite,
            },
            original_name,
        })
    }

    /// Handle mouse events for dragging the popup
    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> bool {
        self
            .popup
            .handle_mouse(mouse_col, mouse_row, mouse_down, area, (POPUP_WIDTH, POPUP_HEIGHT))
    }

    pub fn is_dragging(&self) -> bool {
        self.popup.is_dragging()
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let popup_rect = self.popup.rect(POPUP_WIDTH, POPUP_HEIGHT);
        let title = match self.mode {
            FormMode::Create => " Add Color ",
            FormMode::Edit { .. } => " Edit Color ",
        };

        popup::render_popup_frame(
            buf,
            &self.popup,
            area,
            POPUP_WIDTH,
            POPUP_HEIGHT,
            title,
            theme::popup_background_style(),
            theme::popup_border_style(),
            theme::popup_title_style(),
        );

        let mut y = popup_rect.y + 2;
        let focused = self.focused_field;
        let input_styles = TextInputStyle::new();

        Self::render_text_field(
            focused,
            0,
            "Name:",
            &mut self.name,
            popup_rect.x + 2,
            y,
            popup_rect.width,
            &input_styles,
            buf,
        );
        y += 1;

        Self::render_text_field(
            focused,
            1,
            "Category:",
            &mut self.category,
            popup_rect.x + 2,
            y,
            popup_rect.width,
            &input_styles,
            buf,
        );
        y += 1;

        let color_val = self.color.lines()[0].to_string();
        Self::render_color_field(
            focused,
            2,
            "Color:",
            &mut self.color,
            &color_val,
            popup_rect.x + 2,
            y,
            &input_styles,
            buf,
        );
        y += 1;

        Self::render_favorite_row(
            focused,
            3,
            "Favorite:",
            self.favorite,
            popup_rect.x + 2,
            y,
            popup_rect.width,
            buf,
        );
        y += 2;

        let status = "Tab:Next  Shift+Tab:Prev  Ctrl+S:Save  Esc:Close";
        buf.set_string(popup_rect.x + 2, y, status, theme::status_text_style());
    }

    fn render_text_field(
        focused_field: usize,
        field_id: usize,
        label: &str,
        textarea: &mut TextArea,
        x: u16,
        y: u16,
        width: u16,
        styles: &TextInputStyle,
        buf: &mut Buffer,
    ) {
        let is_focused = focused_field == field_id;
        let label_span = Span::styled(label, theme::label_style(is_focused));
        let label_area = Rect { x, y, width: 14, height: 1 };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        let base_style = styles.base;
        let focused_style = styles.focused;
        textarea.set_style(if is_focused { focused_style } else { base_style });
        textarea.set_cursor_style(styles.cursor);
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(styles.placeholder);

        let input_area = Rect {
            x: x + 10,
            y,
            width: width.saturating_sub(14),
            height: 1,
        };

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
        styles: &TextInputStyle,
        buf: &mut Buffer,
    ) {
        let is_focused = focused_field == field_id;
        let label_span = Span::styled(label, theme::label_style(is_focused));
        let label_area = Rect { x, y, width: 14, height: 1 };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        let base_style = styles.base;
        let focused_style = styles.focused;
        textarea.set_style(if is_focused { focused_style } else { base_style });
        textarea.set_cursor_style(styles.cursor);
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(styles.placeholder);

        let input_area = Rect { x: x + 10, y, width: 10, height: 1 };
        textarea.set_block(Block::default().borders(Borders::NONE).style(base_style));
        RatatuiWidget::render(&*textarea, input_area, buf);

        let preview_x = input_area.x + input_area.width + 1;
        if let Some(color) = Self::parse_color(color_val) {
            let style = Style::default().bg(color);
            buf.set_string(preview_x, y, "    ", style);
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
    ) {
        let is_focused = focused_field == field_id;
        let label_span = Span::styled(label, theme::label_style(is_focused));
        let label_area = Rect { x, y, width: 14, height: 1 };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        let style = Style::default()
            .fg(theme::colors::INPUT_FG)
            .bg(theme::colors::INPUT_BG);

        let val_text = if value { "[X]" } else { "[ ]" };
        buf.set_string(x + 10, y, val_text, style);
    }

    fn is_valid_hex_color(color: &str) -> bool {
        if !color.starts_with('#') || color.len() != 7 {
            return false;
        }
        color[1..].chars().all(|c| c.is_ascii_hexdigit())
    }

    fn parse_color(hex: &str) -> Option<Color> {
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
}

#[derive(Debug, Clone)]
pub enum FormAction {
    Save { color: PaletteColor, original_name: Option<String> },
    Delete,
    Cancel,
    Error(String),
}
