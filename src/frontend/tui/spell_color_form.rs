//! Popup dialog for mapping spell IDs to color ranges.
//!
//! Used by the spell color browser/editor to input ranges, preview swatches,
//! and persist the chosen palette.

use crate::frontend::tui::crossterm_bridge;
use crate::config::SpellColorRange;
use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget as RatatuiWidget},
};
use tui_textarea::TextArea;

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
    popup_position: (u16, u16),
    pub is_dragging: bool,
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
            popup_position: (0, 0),
            is_dragging: false,
            drag_offset: (0, 0),
        }
    }

    pub fn new_edit(index: usize, spell_color: &SpellColorRange) -> Self {
        let spell_ids_str = spell_color
            .spells
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let mut spell_ids = TextArea::default();
        spell_ids.insert_str(&spell_ids_str);

        let mut bar_color = TextArea::default();
        let bar_color_val = spell_color
            .bar_color
            .clone()
            .unwrap_or_else(|| spell_color.color.clone());
        bar_color.insert_str(&bar_color_val);

        let mut text_color = TextArea::default();
        let text_color_val = spell_color
            .text_color
            .clone()
            .unwrap_or_else(|| "#ffffff".to_string());
        text_color.insert_str(&text_color_val);

        let mut bg_color = TextArea::default();
        let bg_color_val = spell_color
            .bg_color
            .clone()
            .unwrap_or_else(|| "#000000".to_string());
        bg_color.insert_str(&bg_color_val);

        Self {
            mode: FormMode::Edit(index),
            focused_field: 0,
            spell_ids,
            bar_color,
            text_color,
            bg_color,
            popup_position: (0, 0),
            is_dragging: false,
            drag_offset: (0, 0),
        }
    }

    pub fn input(&mut self, key: KeyEvent) -> Option<SpellColorFormResult> {
        // Note: All navigation keys (Tab, Shift+Tab, Esc, Enter, Up, Down, Ctrl+S, Ctrl+A)
        // are now routed via MenuAction in mod.rs. This method only handles text input.

        // Pass to the focused textarea (convert KeyEvent for tui-textarea compatibility)
        let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key);
        match self.focused_field {
            0 => {
                self.spell_ids.input(rt_key);
            }
            1 => {
                self.bar_color.input(rt_key);
            }
            2 => {
                self.text_color.input(rt_key);
            }
            3 => {
                self.bg_color.input(rt_key);
            }
            _ => {}
        }

        None
    }

    /// Handle MenuAction (called from mod.rs input routing)
    pub fn handle_action(&mut self, action: crate::core::menu_actions::MenuAction) -> Option<SpellColorFormResult> {
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
            MenuAction::Select => {
                // Enter key - move to next field
                self.next_field();
                None
            }
            MenuAction::Save => {
                // Ctrl+S - save the form
                self.save_internal()
            }
            MenuAction::Delete => {
                // Delete key - delete spell color range (only in edit mode)
                if let FormMode::Edit(index) = self.mode {
                    Some(SpellColorFormResult::Delete(index))
                } else {
                    None
                }
            }
            _ => None
        }
    }

    fn next_field(&mut self) {
        self.focused_field = (self.focused_field + 1) % 4;
    }

    fn previous_field(&mut self) {
        self.focused_field = if self.focused_field == 0 {
            3
        } else {
            self.focused_field - 1
        };
    }

    fn save_internal(&self) -> Option<SpellColorFormResult> {
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

        let spell_color = SpellColorRange {
            spells: spell_ids,
            color: bar_color_str.clone(), // Legacy field
            bar_color: if bar_color_str.is_empty() {
                None
            } else {
                Some(bar_color_str)
            },
            text_color: if text_color_str.is_empty() {
                None
            } else {
                Some(text_color_str)
            },
            bg_color: if bg_color_str.is_empty() {
                None
            } else {
                Some(bg_color_str)
            },
        };

        Some(SpellColorFormResult::Save(spell_color))
    }

    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let popup_width = 52;
        let popup_height = 9;

        // Center on first render
        if self.popup_position == (0, 0) {
            let centered_x = (area.width.saturating_sub(popup_width)) / 2;
            let centered_y = (area.height.saturating_sub(popup_height)) / 2;
            self.popup_position = (centered_x, centered_y);
        }

        let (popup_col, popup_row) = self.popup_position;

        // Parse textarea background color from config
        let textarea_bg = if config.colors.ui.textarea_background == "-" {
            Color::Reset
        } else if let Some(color) = Self::parse_hex_color(&config.colors.ui.textarea_background) {
            color
        } else {
            Color::Rgb(53, 5, 5) // Fallback to maroon
        };

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x: popup_col,
            y: popup_row,
            width: popup_width,
            height: popup_height,
        };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in popup_row..popup_row + popup_height {
            for col in popup_col..popup_col + popup_width {
                if col < area.width && row < area.height {
                    buf.set_string(col, row, " ", Style::default().bg(crossterm_bridge::to_ratatui_color(theme.browser_background)));
                }
            }
        }

        // Draw border
        let title = match self.mode {
            FormMode::Create => " Add Spell Color ",
            FormMode::Edit(_) => " Edit Spell Color ",
        };

        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label));

        // Top border
        let top = format!("┌{}┐", "─".repeat(popup_width as usize - 2));
        buf.set_string(popup_col, popup_row, &top, border_style);

        // Title
        buf.set_string(
            popup_col + 2,
            popup_row,
            title,
            border_style.add_modifier(Modifier::BOLD),
        );

        // Side borders
        for i in 1..popup_height - 1 {
            buf.set_string(popup_col, popup_row + i, "│", border_style);
            buf.set_string(
                popup_col + popup_width - 1,
                popup_row + i,
                "│",
                border_style,
            );
        }

        // Bottom border
        let bottom = format!("└{}┘", "─".repeat(popup_width as usize - 2));
        buf.set_string(
            popup_col,
            popup_row + popup_height - 1,
            &bottom,
            border_style,
        );

        // Get color values before rendering
        let bar_color_val = self.bar_color.lines()[0].to_string();
        let text_color_val = self.text_color.lines()[0].to_string();
        let bg_color_val = self.bg_color.lines()[0].to_string();

        // Render fields
        let mut y = popup_row + 2;
        let focused = self.focused_field;

        // Spell IDs field
        Self::render_text_field(
            focused,
            0,
            "Spell IDs:",
            &mut self.spell_ids,
            popup_col + 2,
            y,
            popup_width - 4,
            buf,
            textarea_bg,
            theme,
        );
        y += 1;

        // Bar Color field (10 chars) + preview
        Self::render_color_field(
            focused,
            1,
            "Bar Color:",
            &mut self.bar_color,
            &bar_color_val,
            popup_col + 2,
            y,
            buf,
            textarea_bg,
            theme,
        );
        y += 1;

        // Text Color field (10 chars) + preview
        Self::render_color_field(
            focused,
            2,
            "Text Color:",
            &mut self.text_color,
            &text_color_val,
            popup_col + 2,
            y,
            buf,
            textarea_bg,
            theme,
        );
        y += 1;

        // Background Color field (10 chars) + preview
        Self::render_color_field(
            focused,
            3,
            "Background:",
            &mut self.bg_color,
            &bg_color_val,
            popup_col + 2,
            y,
            buf,
            textarea_bg,
            theme,
        );
        y += 2;

        // Status bar
        let status = "Tab:Next  Shift+Tab:Prev  Ctrl+s:Save  Esc:Close";
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
        theme: &crate::theme::AppTheme,
    ) {
        let is_focused = focused_field == field_id;
        let label_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Rgb(100, 149, 237))
        };
        let label_span = Span::styled(label, label_style);
        let label_area = Rect {
            x,
            y,
            width: 14,
            height: 1,
        };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        let base_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label)).bg(textarea_bg);
        textarea.set_style(base_style);
        textarea.set_cursor_style(
            Style::default()
                .bg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                .fg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
        );
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(Style::default().fg(Color::Gray).bg(textarea_bg));

        let input_area = Rect {
            x: x + 12,
            y,
            width: width.saturating_sub(12),
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
        buf: &mut Buffer,
        textarea_bg: Color,
        theme: &crate::theme::AppTheme,
    ) {
        let is_focused = focused_field == field_id;
        let label_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Rgb(100, 149, 237))
        };
        let label_span = Span::styled(label, label_style);
        let label_area = Rect {
            x,
            y,
            width: 14,
            height: 1,
        };
        let label_para = Paragraph::new(Line::from(label_span));
        RatatuiWidget::render(label_para, label_area, buf);

        let base_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label)).bg(textarea_bg);
        textarea.set_style(base_style);
        textarea.set_cursor_style(
            Style::default()
                .bg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                .fg(crossterm_bridge::to_ratatui_color(theme.browser_background)),
        );
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_style(Style::default().fg(Color::Gray).bg(textarea_bg));

        let input_area = Rect {
            x: x + 12,
            y,
            width: 10,
            height: 1,
        };
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

// Trait implementations for SpellColorFormWidget
use super::widget_traits::{Cyclable, FieldNavigable, TextEditable, Toggleable};

impl TextEditable for SpellColorFormWidget {
    fn get_focused_field(&self) -> Option<&TextArea<'static>> {
        match self.focused_field {
            0 => Some(&self.spell_ids),
            1 => Some(&self.bar_color),
            2 => Some(&self.text_color),
            3 => Some(&self.bg_color),
            _ => None,
        }
    }

    fn get_focused_field_mut(&mut self) -> Option<&mut TextArea<'static>> {
        match self.focused_field {
            0 => Some(&mut self.spell_ids),
            1 => Some(&mut self.bar_color),
            2 => Some(&mut self.text_color),
            3 => Some(&mut self.bg_color),
            _ => None,
        }
    }
}

impl FieldNavigable for SpellColorFormWidget {
    fn next_field(&mut self) {
        self.next_field();
    }

    fn previous_field(&mut self) {
        self.previous_field();
    }

    fn field_count(&self) -> usize {
        4
    }

    fn current_field(&self) -> usize {
        self.focused_field
    }
}

// Implement Saveable trait for uniform form interface
impl super::widget_traits::Saveable for SpellColorFormWidget {
    type SaveResult = SpellColorFormResult;

    fn try_save(&mut self) -> Option<Self::SaveResult> {
        // Delegate to internal save logic
        self.save_internal()
    }
}

impl Toggleable for SpellColorFormWidget {
    fn toggle_focused(&mut self) -> Option<bool> {
        // No toggleable fields in SpellColorFormWidget
        None
    }
}

impl Cyclable for SpellColorFormWidget {
    fn cycle_forward(&mut self) {
        // No cyclable fields in SpellColorFormWidget
    }

    fn cycle_backward(&mut self) {
        // No cyclable fields in SpellColorFormWidget
    }
}
