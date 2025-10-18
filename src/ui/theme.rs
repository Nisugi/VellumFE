use ratatui::style::{Color, Modifier, Style};

/// Shared color palette used by the modernized UI editors.
pub mod colors {
    use ratatui::style::Color;

    pub const POPUP_BACKGROUND: Color = Color::Black;
    pub const BORDER: Color = Color::Cyan;
    pub const TITLE: Color = Color::Cyan;
    pub const STATUS_TEXT: Color = Color::Gray;

    pub const LABEL_FOCUSED: Color = Color::Yellow;
    pub const LABEL_DEFAULT: Color = Color::Rgb(100, 149, 237);

    pub const INPUT_FG: Color = Color::Cyan;
    pub const INPUT_BG: Color = Color::Rgb(53, 5, 5);
    pub const INPUT_FOCUSED_FG: Color = Color::Black;
    pub const INPUT_FOCUSED_BG: Color = Color::Rgb(255, 215, 0);

    pub const CURSOR_BG: Color = Color::White;
    pub const CURSOR_FG: Color = Color::Black;
}

/// Styling bundle for text inputs.
pub struct TextInputStyle {
    pub base: Style,
    pub focused: Style,
    pub placeholder: Style,
    pub cursor: Style,
}

impl TextInputStyle {
    pub fn new() -> Self {
        let base = Style::default()
            .fg(colors::INPUT_FG)
            .bg(colors::INPUT_BG);

        let focused = Style::default()
            .fg(colors::INPUT_FOCUSED_FG)
            .bg(colors::INPUT_FOCUSED_BG)
            .add_modifier(Modifier::BOLD);

        let placeholder = Style::default()
            .fg(Color::Gray)
            .bg(colors::INPUT_BG);

        let cursor = Style::default()
            .bg(colors::CURSOR_BG)
            .fg(colors::CURSOR_FG);

        Self {
            base,
            focused,
            placeholder,
            cursor,
        }
    }
}

pub fn popup_border_style() -> Style {
    Style::default().fg(colors::BORDER)
}

pub fn popup_title_style() -> Style {
    popup_border_style().add_modifier(Modifier::BOLD)
}

pub fn popup_background_style() -> Style {
    Style::default().bg(colors::POPUP_BACKGROUND)
}

pub fn label_style(is_focused: bool) -> Style {
    if is_focused {
        Style::default().fg(colors::LABEL_FOCUSED)
    } else {
        Style::default().fg(colors::LABEL_DEFAULT)
    }
}

pub fn status_text_style() -> Style {
    Style::default().fg(colors::STATUS_TEXT)
}
