use crate::performance::PerformanceStats;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Widget},
};

pub struct PerformanceStatsWidget {
    show_border: bool,
    border_color: Color,
    background_color: Option<Color>,
}

impl PerformanceStatsWidget {
    pub fn new() -> Self {
        Self {
            show_border: true,
            border_color: Color::Gray,
            background_color: Some(Color::Black),  // Default black background
        }
    }

    pub fn with_border(mut self, show: bool) -> Self {
        self.show_border = show;
        self
    }

    pub fn with_border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    pub fn with_background_color(mut self, color: Option<Color>) -> Self {
        self.background_color = color;
        self
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, stats: &PerformanceStats) {
        // Fill background if specified (clear text behind widget)
        if let Some(bg_color) = self.background_color {
            for y in area.y..area.y + area.height {
                for x in area.x..area.x + area.width {
                    if x < buf.area().width && y < buf.area().height {
                        buf[(x, y)].set_char(' ').set_bg(bg_color).set_fg(Color::Reset);
                    }
                }
            }
        }

        // Create block with border if enabled
        let block = if self.show_border {
            Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(self.border_color))
                .title("Performance Stats")
        } else {
            Block::default()
        };

        let inner = block.inner(area);
        block.render(area, buf);

        // Format stats into lines
        let lines = vec![
            // Frame stats
            Line::from(vec![
                Span::styled("FPS: ", Style::default().fg(Color::Cyan)),
                Span::styled(format!("{:.1}", stats.fps()), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Frame: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{:.2}ms (max: {:.2})",
                        stats.avg_frame_time_ms(),
                        stats.max_frame_time_ms()
                    ),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("Render: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{:.2}ms (max: {:.2})",
                        stats.avg_render_time_ms(),
                        stats.max_render_time_ms()
                    ),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("UI: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{:.2}ms", stats.avg_ui_render_time_ms()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("Wrap: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{:.0}μs", stats.avg_text_wrap_time_us()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(""),
            // Network stats
            Line::from(vec![
                Span::styled("Net In: ", Style::default().fg(Color::Green)),
                Span::styled(
                    format!("{:.2} KB/s", stats.bytes_received_per_sec() as f64 / 1024.0),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("Net Out: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{:.2} KB/s", stats.bytes_sent_per_sec() as f64 / 1024.0),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(""),
            // Parser stats
            Line::from(vec![
                Span::styled("Parse: ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    format!("{:.0}μs", stats.avg_parse_time_us()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("Chunks/s: ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    format!("{}", stats.chunks_per_sec()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("Elems/s: ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    format!("{}", stats.elements_per_sec()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(""),
            // Event stats
            Line::from(vec![
                Span::styled("Event: ", Style::default().fg(Color::LightBlue)),
                Span::styled(
                    format!("{:.0}μs (max: {:.0})",
                        stats.avg_event_process_time_us(),
                        stats.max_event_process_time_us()
                    ),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(""),
            // Memory stats
            Line::from(vec![
                Span::styled("Memory: ", Style::default().fg(Color::Red)),
                Span::styled(
                    format!("{:.1} MB", stats.estimated_memory_mb()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("Lines: ", Style::default().fg(Color::Red)),
                Span::styled(
                    format!("{}", stats.total_lines_buffered()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(vec![
                Span::styled("Windows: ", Style::default().fg(Color::Red)),
                Span::styled(
                    format!("{}", stats.active_window_count()),
                    Style::default().fg(Color::White)
                ),
            ]),
            Line::from(""),
            // Uptime
            Line::from(vec![
                Span::styled("Uptime: ", Style::default().fg(Color::Blue)),
                Span::styled(stats.uptime_formatted(), Style::default().fg(Color::White)),
            ]),
        ];

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

impl Default for PerformanceStatsWidget {
    fn default() -> Self {
        Self::new()
    }
}
