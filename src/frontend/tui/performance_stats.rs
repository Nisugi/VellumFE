//! Terminal performance widget with configurable sections and theming.
//!
//! Rows derive from the shared [`PERF_METRICS`] table filtered to the TUI
//! scope, so this widget can only show metrics the TUI actually records.
//! Threshold coloring and block-character sparklines come from the same
//! table; per-row visibility follows the `ui.perf_show_*` settings.

use crate::config::{BorderSides, PerformanceWidgetData};
use crate::frontend::tui::colors::parse_color_to_ratatui;
use crate::frontend::tui::crossterm_bridge;
use crate::performance::{
    sparkline_string, PerfFrontend, PerfMetric, PerfSeverity, PerformanceStats, PERF_METRICS,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Widget},
};

/// Sparkline width in cells, drawn after the first line of a row.
const SPARK_WIDTH: usize = 12;

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
    flags: PerformanceWidgetData,
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
            flags: PerformanceWidgetData::default(),
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

    pub fn apply_flags(&mut self, data: &PerformanceWidgetData) {
        self.flags = data.clone();
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
                                buf[(x, y)].set_char(' ').set_bg(color).set_fg(Color::Reset);
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
        let spark_color = self.themed_color(Color::DarkGray);

        if !self.flags.enabled {
            let paragraph = Paragraph::new(Line::from(vec![Span::styled(
                "Monitoring disabled",
                Style::default().fg(label_color),
            )]));
            paragraph.render(inner, buf);
            return;
        }

        let metrics: Vec<&PerfMetric> = PERF_METRICS
            .iter()
            .filter(|metric| metric.in_scope(PerfFrontend::Tui))
            .filter(|metric| metric.enabled_in(&self.flags))
            .collect();

        let mut lines: Vec<Line> = Vec::new();
        for metric in metrics {
            let row_color = match metric.severity.map(|f| f(stats)) {
                Some(PerfSeverity::Crit) => Color::Red,
                Some(PerfSeverity::Warn) => Color::Yellow,
                _ => value_color,
            };
            let value = (metric.format)(stats);
            for (i, text) in value.lines().enumerate() {
                let mut spans: Vec<Span> = Vec::new();
                if i == 0 {
                    spans.push(Span::styled(
                        format!("{:<8}", metric.label),
                        Style::default().fg(label_color),
                    ));
                } else {
                    spans.push(Span::raw("        "));
                }
                spans.push(Span::styled(
                    text.to_string(),
                    Style::default().fg(row_color),
                ));
                if i == 0 && self.flags.sparklines {
                    if let Some(spark) = metric.spark {
                        let s = sparkline_string(&spark(stats), SPARK_WIDTH);
                        if !s.is_empty() {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(s, Style::default().fg(spark_color)));
                        }
                    }
                }
                lines.push(Line::from(spans));
            }
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
