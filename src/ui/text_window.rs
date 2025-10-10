use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};
use std::collections::VecDeque;

#[derive(Clone)]
pub struct StyledText {
    pub content: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

// One display line (post-wrapping) with multiple styled spans
#[derive(Clone)]
struct WrappedLine {
    spans: Vec<(String, Style)>,
}

// One logical line (before wrapping) - stores original styled content
#[derive(Clone)]
struct LogicalLine {
    spans: Vec<(String, Style)>,
}

pub struct TextWindow {
    // Store original logical lines (for re-wrapping)
    logical_lines: VecDeque<LogicalLine>,
    // Cached wrapped lines (invalidated when width changes)
    wrapped_lines: VecDeque<WrappedLine>,
    // Accumulate styled chunks for current logical line
    current_line_spans: Vec<(String, Style)>,
    max_lines: usize,
    scroll_offset: usize,
    title: String,
    last_width: u16,
    needs_rewrap: bool, // Flag to trigger re-wrapping
    // Border configuration
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
}

impl TextWindow {
    pub fn new(title: impl Into<String>, max_lines: usize) -> Self {
        Self {
            logical_lines: VecDeque::with_capacity(max_lines),
            wrapped_lines: VecDeque::with_capacity(max_lines * 2), // More space for wrapped
            current_line_spans: Vec::new(),
            max_lines,
            scroll_offset: 0,
            title: title.into(),
            last_width: 0,
            needs_rewrap: false,
            show_border: true,
            border_style: None,
            border_color: None,
        }
    }

    pub fn with_border_config(
        mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) -> Self {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color;
        self
    }

    /// Update border configuration on an existing window
    pub fn set_border_config(
        &mut self,
        show_border: bool,
        border_style: Option<String>,
        border_color: Option<String>,
    ) {
        self.show_border = show_border;
        self.border_style = border_style;
        self.border_color = border_color;
    }

    /// Update the window title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn add_text(&mut self, styled: StyledText) {
        let style = Style::default()
            .fg(styled.fg.unwrap_or(Color::Gray))
            .bg(styled.bg.unwrap_or(Color::Reset))
            .add_modifier(if styled.bold {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });

        // Add this styled chunk to current line
        self.current_line_spans.push((styled.content, style));
    }

    pub fn finish_line(&mut self, _width: u16) {
        if self.current_line_spans.is_empty() {
            return;
        }

        // Store the original logical line
        let logical_line = LogicalLine {
            spans: self.current_line_spans.clone(),
        };
        self.logical_lines.push_back(logical_line);

        // Remove oldest logical line if we exceed buffer
        if self.logical_lines.len() > self.max_lines {
            self.logical_lines.pop_front();
        }

        // Wrap this logical line and add to wrapped cache
        let actual_width = if self.last_width > 0 {
            self.last_width
        } else {
            80 // Fallback
        };

        let wrapped = self.wrap_styled_spans(&self.current_line_spans, actual_width as usize);

        // Add wrapped lines to the END
        for line in wrapped {
            self.wrapped_lines.push_back(line);
        }

        self.current_line_spans.clear();
    }

    // Wrap a series of styled spans into multiple display lines
    fn wrap_styled_spans(&self, spans: &[(String, Style)], width: usize) -> Vec<WrappedLine> {
        if width == 0 {
            return vec![];
        }

        let mut result = Vec::new();
        let mut current_line_spans: Vec<(String, Style)> = Vec::new();
        let mut current_line_len = 0;

        for (text, style) in spans {
            // Process text character by character to preserve exact spacing
            let mut chars = text.chars().peekable();
            let mut word_buffer = String::new();

            while let Some(ch) = chars.next() {
                if ch.is_whitespace() {
                    // Flush word buffer if we have one
                    if !word_buffer.is_empty() {
                        let word_len = word_buffer.len();

                        // Check if we need to wrap before adding this word
                        if current_line_len > 0 && current_line_len + 1 + word_len > width {
                            // Wrap to new line
                            result.push(WrappedLine {
                                spans: current_line_spans.clone(),
                            });
                            current_line_spans.clear();
                            current_line_len = 0;
                        }

                        // Add word to current line
                        if current_line_len > 0 {
                            // Check if word starts with punctuation (don't add space before punctuation)
                            let needs_space = !word_buffer.starts_with(|c: char| c.is_ascii_punctuation());

                            // Append to last span if same style
                            if let Some((last_text, last_style)) = current_line_spans.last_mut() {
                                if last_style == style {
                                    if needs_space {
                                        last_text.push(' ');
                                    }
                                    last_text.push_str(&word_buffer);
                                } else {
                                    if needs_space {
                                        current_line_spans.push((format!(" {}", word_buffer), *style));
                                    } else {
                                        current_line_spans.push((word_buffer.clone(), *style));
                                    }
                                }
                            } else {
                                if needs_space {
                                    current_line_spans.push((format!(" {}", word_buffer), *style));
                                } else {
                                    current_line_spans.push((word_buffer.clone(), *style));
                                }
                            }
                            current_line_len += if needs_space { 1 } else { 0 } + word_len;
                        } else {
                            current_line_spans.push((word_buffer.clone(), *style));
                            current_line_len += word_len;
                        }
                        word_buffer.clear();
                    }
                    // Skip trailing whitespace (don't add to output)
                } else {
                    word_buffer.push(ch);
                }
            }

            // Flush any remaining word
            if !word_buffer.is_empty() {
                let word_len = word_buffer.len();
                let needs_space = !word_buffer.starts_with(|c: char| c.is_ascii_punctuation());
                let space_len = if needs_space { 1 } else { 0 };

                if current_line_len > 0 && current_line_len + space_len + word_len > width {
                    result.push(WrappedLine {
                        spans: current_line_spans.clone(),
                    });
                    current_line_spans.clear();
                    current_line_len = 0;
                }

                if current_line_len > 0 {
                    if let Some((last_text, last_style)) = current_line_spans.last_mut() {
                        if last_style == style {
                            if needs_space {
                                last_text.push(' ');
                            }
                            last_text.push_str(&word_buffer);
                        } else {
                            if needs_space {
                                current_line_spans.push((format!(" {}", word_buffer), *style));
                            } else {
                                current_line_spans.push((word_buffer.clone(), *style));
                            }
                        }
                    } else {
                        if needs_space {
                            current_line_spans.push((format!(" {}", word_buffer), *style));
                        } else {
                            current_line_spans.push((word_buffer.clone(), *style));
                        }
                    }
                    current_line_len += space_len + word_len;
                } else {
                    current_line_spans.push((word_buffer.clone(), *style));
                    current_line_len += word_len;
                }
            }
        }

        // Push any remaining content
        if !current_line_spans.is_empty() {
            result.push(WrappedLine {
                spans: current_line_spans,
            });
        }

        // Handle empty line case
        if result.is_empty() {
            result.push(WrappedLine {
                spans: vec![("".to_string(), Style::default())],
            });
        }

        result
    }

    pub fn scroll_up(&mut self, amount: usize) {
        // Scrolling up = viewing older lines = increase offset from bottom
        // scroll_offset = how many lines back from the end we are
        let max_offset = self.wrapped_lines.len();
        self.scroll_offset = self.scroll_offset.saturating_add(amount).min(max_offset);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        // Scrolling down = viewing newer lines = decrease offset
        // offset 0 = at the bottom, viewing live
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn set_width(&mut self, width: u16) {
        if width == self.last_width || width == 0 {
            return;
        }

        self.last_width = width;
        self.needs_rewrap = true; // Mark that we need to re-wrap all lines
    }

    /// Re-wrap all logical lines with the current width
    fn rewrap_all(&mut self) {
        self.wrapped_lines.clear();

        let width = if self.last_width > 0 {
            self.last_width as usize
        } else {
            80
        };

        // Wrap each logical line
        for logical_line in &self.logical_lines {
            let wrapped = self.wrap_styled_spans(&logical_line.spans, width);
            for line in wrapped {
                self.wrapped_lines.push_back(line);
            }
        }

        self.needs_rewrap = false;
    }
}

impl TextWindow {
    /// Render the window with optional focus indicator
    pub fn render_with_focus(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        // Update width for wrapping
        let inner_width = area.width.saturating_sub(2); // Account for borders
        self.set_width(inner_width);

        // Re-wrap all lines if width changed
        if self.needs_rewrap {
            self.rewrap_all();
        }

        // Build visible lines for display
        // Buffer storage: wrapped_lines[0] = oldest, wrapped_lines[end] = newest
        // Display: oldest at top, newest at bottom (standard chat/log view)
        // scroll_offset = how many lines back from the end we're viewing
        // scroll_offset=0 means viewing the bottom (live, newest lines)
        // scroll_offset>0 means scrolled back to view older lines

        let visible_height = area.height.saturating_sub(2) as usize; // Account for borders
        let total_lines = self.wrapped_lines.len();

        if total_lines == 0 {
            // No lines to display
            let paragraph = Paragraph::new(vec![])
                .block(
                    if focused {
                        Block::default()
                            .title(self.title.as_str())
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    } else {
                        Block::default()
                            .title(self.title.as_str())
                            .borders(Borders::ALL)
                    }
                );
            paragraph.render(area, buf);
            return;
        }

        // Calculate which lines to display
        // We want to show the last visible_height lines, minus scroll_offset
        // Example: 100 total lines, visible_height=20, scroll_offset=0
        //   -> show lines 80-99 (the last 20)
        // Example: 100 total lines, visible_height=20, scroll_offset=10
        //   -> show lines 70-89 (20 lines, but 10 back from the end)

        let end_line = total_lines.saturating_sub(self.scroll_offset);
        let start_line = end_line.saturating_sub(visible_height);

        // Collect lines from buffer (oldest to newest order)
        let mut display_lines: Vec<Line> = Vec::new();
        for idx in start_line..end_line {
            if let Some(wrapped) = self.wrapped_lines.get(idx) {
                let spans: Vec<Span> = wrapped
                    .spans
                    .iter()
                    .map(|(text, style)| Span::styled(text.clone(), *style))
                    .collect();
                display_lines.push(Line::from(spans));
            }
        }

        // Lines are already in the correct order (oldest at top, newest at bottom)
        // No need to reverse!

        // Build block with focus indicator and scroll position
        // Show "[scrolled back X lines]" when not at bottom
        let title = if self.scroll_offset > 0 {
            format!("{} [↑{}]", self.title, self.scroll_offset)
        } else {
            self.title.clone()
        };

        // Create block based on border configuration
        let mut block = if self.show_border {
            Block::default()
                .title(title.as_str())
                .borders(Borders::ALL)
        } else {
            Block::default() // No borders
        };

        // Apply border style if specified
        if let Some(ref style_name) = self.border_style {
            let border_type = match style_name.as_str() {
                "double" => BorderType::Double,
                "rounded" => BorderType::Rounded,
                "thick" => BorderType::Thick,
                _ => BorderType::Plain, // "single" or default
            };
            block = block.border_type(border_type);
        }

        // Apply border color
        let mut border_style = Style::default();
        if let Some(ref color_hex) = self.border_color {
            if let Some(color) = Self::parse_hex_color(color_hex) {
                border_style = border_style.fg(color);
            }
        }

        // Override with focus color if focused
        if focused {
            border_style = border_style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
        }

        if self.show_border {
            block = block.border_style(border_style);
        }

        let paragraph = Paragraph::new(display_lines).block(block);
        paragraph.render(area, buf);
    }

    fn parse_hex_color(hex: &str) -> Option<Color> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    }
}

impl Widget for &mut TextWindow {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_with_focus(area, buf, false);
    }
}
