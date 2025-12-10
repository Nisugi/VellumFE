//! Terminal performance widget with configurable sections and theming.
//!
//! This widget mirrors the window/editor options: border style/sides, optional
//! background fill, and per-section toggles so the user can pick which metrics
//! to surface. Metrics are pulled from `PerformanceStats` each render.

use crate::config::BorderSides;
use crate::frontend::tui::colors::parse_color_to_ratatui;
use crate::frontend::tui::crossterm_bridge;
use crate::performance::PerformanceStats;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Widget},
};

#[derive(Clone)]
pub struct PerformanceStatsWidget {
    title: String,
    show_border: bool,
    border_style: Option<String>,
    border_color: Option<String>,
    border_sides: BorderSides,
    background_color: Option<String>,
    transparent_background: bool,
    text_color: Option<String>,
    // Section toggles
    enabled: bool,
    show_fps: bool,
    show_frame_times: bool,
    show_render_times: bool,
    show_ui_times: bool,
    show_wrap_times: bool,
    show_net: bool,
    show_parse: bool,
    show_events: bool,
    show_memory: bool,
    show_lines: bool,
    show_uptime: bool,
    show_jitter: bool,
    show_frame_spikes: bool,
    show_event_lag: bool,
    show_memory_delta: bool,
}

impl PerformanceStatsWidget {
    pub fn new() -> Self {
        Self {
            title: "Performance".to_string(),
            show_border: true,
            border_style: Some("single".to_string()),
            border_color: None,
            border_sides: BorderSides::default(),
            background_color: None,
            transparent_background: false,
            text_color: None,
            enabled: true,
            show_fps: true,
            show_frame_times: true,
            show_render_times: true,
            show_ui_times: true,
            show_wrap_times: true,
            show_net: true,
            show_parse: true,
            show_events: true,
            show_memory: true,
            show_lines: true,
            show_uptime: true,
            show_jitter: true,
            show_frame_spikes: true,
            show_event_lag: true,
            show_memory_delta: true,
        }
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn set_border_config(&mut self, show: bool, style: Option<String>, color: Option<String>) {
        self.show_border = show;
        self.border_style = style;
        self.border_color = color;
    }

    pub fn set_border_sides(&mut self, sides: BorderSides) {
        self.border_sides = sides;
    }

    pub fn set_background_color(&mut self, color: Option<String>) {
        self.background_color = color;
    }

    pub fn set_transparent_background(&mut self, transparent: bool) {
        self.transparent_background = transparent;
    }

    pub fn set_text_color(&mut self, color: Option<String>) {
        self.text_color = color;
    }

    pub fn apply_flags(&mut self, data: &crate::config::PerformanceWidgetData) {
        self.enabled = data.enabled;
        self.show_fps = data.show_fps;
        self.show_frame_times = data.show_frame_times;
        self.show_render_times = data.show_render_times;
        self.show_ui_times = data.show_ui_times;
        self.show_wrap_times = data.show_wrap_times;
        self.show_net = data.show_net;
        self.show_parse = data.show_parse;
        self.show_events = data.show_events;
        self.show_memory = data.show_memory;
        self.show_lines = data.show_lines;
        self.show_uptime = data.show_uptime;
        self.show_jitter = data.show_jitter;
        self.show_frame_spikes = data.show_frame_spikes;
        self.show_event_lag = data.show_event_lag;
        self.show_memory_delta = data.show_memory_delta;
    }

    fn parse_color(input: &str) -> Option<Color> {
        parse_color_to_ratatui(input)
    }

    fn themed_color(&self, fallback: Color) -> Color {
        self.text_color
            .as_ref()
            .and_then(|c| Self::parse_color(c))
            .unwrap_or(fallback)
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, stats: &PerformanceStats) {
        // Fill background when requested
        if !self.transparent_background {
            if let Some(bg_hex) = &self.background_color {
                if let Some(color) = Self::parse_color(bg_hex) {
                    for y in area.y..area.y.saturating_add(area.height) {
                        for x in area.x..area.x.saturating_add(area.width) {
                            if x < buf.area().width && y < buf.area().height {
                                buf[(x, y)]
                                    .set_char(' ')
                                    .set_bg(color)
                                    .set_fg(Color::Reset);
                            }
                        }
                    }
                }
            }
        }

        // Build outer block
        let mut block = if self.show_border {
            let borders = crossterm_bridge::to_ratatui_borders(&self.border_sides);
            Block::default().borders(borders).title(self.title.as_str())
        } else {
            Block::default()
        };

        if let Some(style_name) = self.border_style.as_deref() {
            let border_type = match style_name {
                "double" => BorderType::Double,
                "rounded" => BorderType::Rounded,
                "thick" => BorderType::Thick,
                "quadrant_inside" => BorderType::QuadrantInside,
                "quadrant_outside" => BorderType::QuadrantOutside,
                _ => BorderType::Plain,
            };
            block = block.border_type(border_type);
        }

        if let Some(color_hex) = &self.border_color {
            if let Some(color) = Self::parse_color(color_hex) {
                block = block.border_style(Style::default().fg(color));
            }
        }

        let inner = block.inner(area);
        block.render(area, buf);

        let label_color = self.themed_color(Color::Cyan);
        let value_color = self.themed_color(Color::White);

        if !self.enabled {
            let paragraph = Paragraph::new(Line::from(vec![Span::styled(
                "Monitoring disabled",
                Style::default().fg(label_color),
            )]));
            paragraph.render(inner, buf);
            return;
        }

        let mut lines: Vec<Line> = Vec::new();
        let add_spacer = |lines: &mut Vec<Line>| {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
        };

        if self.show_fps {
            lines.push(Line::from(vec![
                Span::styled("FPS: ", Style::default().fg(label_color)),
                Span::styled(format!("{:.1}", stats.fps()), Style::default().fg(value_color)),
            ]));
        }
        // Frame/Jitter/Spikes are more meaningful for a fixed-rate GUI; omit in TUI to avoid noisy idle values.
        if self.show_render_times {
            lines.push(Line::from(vec![
                Span::styled("Render: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.2}ms (max {:.2})", stats.avg_render_time_ms(), stats.max_render_time_ms()),
                    Style::default().fg(value_color),
                ),
            ]));
        }
        if self.show_ui_times {
            lines.push(Line::from(vec![
                Span::styled("UI: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.2}ms", stats.avg_ui_render_time_ms()),
                    Style::default().fg(value_color),
                ),
            ]));
        }
        if self.show_wrap_times {
            lines.push(Line::from(vec![
                Span::styled("Wrap: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.0}µs", stats.avg_text_wrap_time_us()),
                    Style::default().fg(value_color),
                ),
            ]));
        }

        if self.show_net {
            add_spacer(&mut lines);
            lines.push(Line::from(vec![
                Span::styled("Net In: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.2} KB/s", stats.bytes_received_per_sec() as f64 / 1024.0),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Net Out: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.2} KB/s", stats.bytes_sent_per_sec() as f64 / 1024.0),
                    Style::default().fg(value_color),
                ),
            ]));
        }

        if self.show_parse {
            add_spacer(&mut lines);
            lines.push(Line::from(vec![
                Span::styled("Parse: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.0}µs", stats.avg_parse_time_us()),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Chunks/s: ", Style::default().fg(label_color)),
                Span::styled(
                    stats.chunks_per_sec().to_string(),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Elems/s: ", Style::default().fg(label_color)),
                Span::styled(
                    stats.elements_per_sec().to_string(),
                    Style::default().fg(value_color),
                ),
            ]));
        }

        if self.show_events {
            add_spacer(&mut lines);
            lines.push(Line::from(vec![
                Span::styled("Event: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.0}us", stats.avg_event_process_time_us()),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Event Max: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.0}us", stats.max_event_process_time_us()),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Queue Max: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{}", stats.max_event_queue_depth()),
                    Style::default().fg(value_color),
                ),
            ]));
        }
        if self.show_memory {
            add_spacer(&mut lines);
            lines.push(Line::from(vec![
                Span::styled("CPU: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.1}% (sys {:.1}%)", stats.process_cpu_percent(), stats.system_cpu_percent()),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("RSS: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.1} MB (virt {:.1} MB)", stats.process_rss_mb(), stats.process_virt_mb()),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Memory: ", Style::default().fg(label_color)),
                Span::styled(
                    format!("{:.1} MB", stats.estimated_memory_mb()),
                    Style::default().fg(value_color),
                ),
            ]));
        }

        if self.show_lines {
            lines.push(Line::from(vec![
                Span::styled("Lines: ", Style::default().fg(label_color)),
                Span::styled(
                    stats.total_lines_buffered().to_string(),
                    Style::default().fg(value_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Windows: ", Style::default().fg(label_color)),
                Span::styled(
                    stats.active_window_count().to_string(),
                    Style::default().fg(value_color),
                ),
            ]));
        }

        if self.show_uptime {
            add_spacer(&mut lines);
            lines.push(Line::from(vec![
                Span::styled("Uptime: ", Style::default().fg(label_color)),
                Span::styled(stats.uptime_formatted(), Style::default().fg(value_color)),
            ]));
        }

        if lines.is_empty() {
            lines.push(Line::from("No metrics enabled"));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

impl Default for PerformanceStatsWidget {
    fn default() -> Self {
        Self::new()
    }
}
