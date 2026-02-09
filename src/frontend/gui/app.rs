use crate::core::AppCore;
use crate::data::{SpanType, StyledLine, TabbedTextContent, WindowContent};
use crate::network::{LichConnection, RawLogger, ServerMessage};
use anyhow::{anyhow, Context, Result};
use eframe::egui;
use eframe::egui::text::LayoutJob;
use eframe::egui::{Color32, FontFamily, FontId, RichText, TextFormat, ViewportBuilder};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::mpsc;

const INITIAL_LAYOUT_WIDTH: u16 = 160;
const INITIAL_LAYOUT_HEIGHT: u16 = 50;
const MAX_RENDERED_LINES: usize = 2000;
const DEFAULT_FONT_SIZE: f32 = 14.0;

pub struct VellumGuiApp {
    app_core: AppCore,
    _runtime: tokio::runtime::Runtime,
    command_tx: mpsc::UnboundedSender<String>,
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    network_handle: Option<tokio::task::JoinHandle<()>>,
    command_input: String,
    close_requested: bool,
}

impl VellumGuiApp {
    pub fn new(
        mut app_core: AppCore,
        login_key: Option<String>,
        initial_width: f32,
        initial_height: f32,
    ) -> Result<Self> {
        app_core.init_windows(
            initial_width.max(1.0) as u16,
            initial_height.max(1.0) as u16,
        );

        let runtime = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let (server_tx, server_rx) = mpsc::unbounded_channel::<ServerMessage>();
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();

        let host = app_core.config.connection.host.clone();
        let port = app_core.config.connection.port;

        let raw_logger = match RawLogger::new(&app_core.config) {
            Ok(logger) => logger,
            Err(err) => {
                tracing::error!("Failed to initialize raw logger: {}", err);
                None
            }
        };

        let network_handle = runtime.spawn(async move {
            if let Err(err) =
                LichConnection::start(&host, port, login_key, server_tx, command_rx, raw_logger)
                    .await
            {
                tracing::error!("GUI network connection error: {}", err);
            }
        });

        Ok(Self {
            app_core,
            _runtime: runtime,
            command_tx,
            server_rx,
            network_handle: Some(network_handle),
            command_input: String::new(),
            close_requested: false,
        })
    }

    fn pump_server_messages(&mut self) {
        while let Ok(message) = self.server_rx.try_recv() {
            match message {
                ServerMessage::Text(line) => {
                    self.app_core
                        .perf_stats
                        .record_bytes_received((line.len() + 1) as u64);
                    if let Err(err) = self.app_core.process_server_data(&line) {
                        self.app_core
                            .add_system_message(&format!("GUI parse error: {}", err));
                    }
                    self.app_core.needs_render = true;
                }
                ServerMessage::Connected => {
                    self.app_core.game_state.connected = true;
                    self.app_core.needs_render = true;
                }
                ServerMessage::Disconnected => {
                    self.app_core.game_state.connected = false;
                    self.app_core.needs_render = true;
                }
            }
        }
    }

    fn submit_command(&mut self) {
        let input = std::mem::take(&mut self.command_input);
        let command = input.trim_end().to_string();
        if command.is_empty() {
            return;
        }

        match self.app_core.send_command(command) {
            Ok(outbound) => {
                if Self::should_send_to_network(&outbound) {
                    self.app_core
                        .perf_stats
                        .record_bytes_sent((outbound.len() + 1) as u64);
                    let _ = self.command_tx.send(outbound);
                }
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Command error: {}", err));
            }
        }

        if !self.app_core.running {
            self.close_requested = true;
        }
    }

    fn should_send_to_network(command: &str) -> bool {
        !command.is_empty()
            && !command.starts_with("__")
            && !command.starts_with("action:")
            && !command.starts_with("menu:")
    }

    fn primary_text_lines(&self) -> Option<&VecDeque<StyledLine>> {
        if let Some(main_window) = self.app_core.ui_state.windows.get("main") {
            match &main_window.content {
                WindowContent::Text(content) => return Some(&content.lines),
                WindowContent::TabbedText(tabbed) => {
                    return Self::find_main_tab(tabbed).map(|tab| &tab.content.lines);
                }
                _ => {}
            }
        }

        for window in self.app_core.ui_state.windows.values() {
            match &window.content {
                WindowContent::Text(content) => {
                    let has_main_stream = content
                        .streams
                        .iter()
                        .any(|stream| stream.eq_ignore_ascii_case("main"));
                    if has_main_stream || content.streams.is_empty() {
                        return Some(&content.lines);
                    }
                }
                WindowContent::TabbedText(tabbed) => {
                    if let Some(tab) = Self::find_main_tab(tabbed) {
                        return Some(&tab.content.lines);
                    }
                }
                _ => {}
            }
        }

        self.app_core
            .ui_state
            .windows
            .values()
            .find_map(|window| match &window.content {
                WindowContent::Text(content) => Some(&content.lines),
                WindowContent::TabbedText(tabbed) => tabbed
                    .tabs
                    .get(tabbed.active_tab_index)
                    .map(|tab| &tab.content.lines),
                _ => None,
            })
    }

    fn find_main_tab(tabbed: &TabbedTextContent) -> Option<&crate::data::TabState> {
        tabbed.tabs.iter().find(|tab| {
            tab.definition
                .streams
                .iter()
                .any(|stream| stream.eq_ignore_ascii_case("main"))
        })
    }

    fn line_to_layout_job(line: &StyledLine, visuals: &egui::Visuals) -> LayoutJob {
        let mut job = LayoutJob::default();
        for segment in &line.segments {
            let foreground = segment
                .fg
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(visuals.text_color());
            let background = segment
                .bg
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(Color32::TRANSPARENT);

            let mut format = TextFormat {
                font_id: FontId::new(
                    DEFAULT_FONT_SIZE + if segment.bold { 0.5 } else { 0.0 },
                    if segment.mono {
                        FontFamily::Monospace
                    } else {
                        FontFamily::Proportional
                    },
                ),
                color: foreground,
                background,
                ..Default::default()
            };

            if matches!(segment.span_type, SpanType::Link) {
                format.underline = egui::Stroke::new(1.0, foreground);
            }

            job.append(&segment.text, 0.0, format);
        }
        job
    }

    fn render_main_text(&self, ui: &mut egui::Ui) {
        let visuals = ui.visuals().clone();
        match self.primary_text_lines() {
            Some(lines) => {
                let start = lines.len().saturating_sub(MAX_RENDERED_LINES);
                for line in lines.iter().skip(start) {
                    let job = Self::line_to_layout_job(line, &visuals);
                    ui.label(job);
                }
            }
            None => {
                ui.label("No text window is configured for the main stream.");
            }
        }
    }
}

impl eframe::App for VellumGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_server_messages();

        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::TopBottomPanel::top("gui_header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("VellumFE GUI");
                let connection_text = if self.app_core.game_state.connected {
                    RichText::new("Connected").color(Color32::from_rgb(0x3a, 0xc5, 0x6d))
                } else {
                    RichText::new("Disconnected").color(Color32::from_rgb(0xd9, 0x55, 0x55))
                };
                ui.separator();
                ui.label(connection_text);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_label(true, "Main");
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("main_text_scroll")
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.render_main_text(ui);
                });
        });

        egui::TopBottomPanel::bottom("gui_command_input").show(ctx, |ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.command_input)
                    .hint_text("Enter command...")
                    .desired_width(f32::INFINITY),
            );

            let pressed_enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.lost_focus() && pressed_enter {
                self.submit_command();
                response.request_focus();
            }
        });

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl Drop for VellumGuiApp {
    fn drop(&mut self) {
        if let Some(handle) = self.network_handle.take() {
            handle.abort();
        }
    }
}

pub fn run_native_gui(app_core: AppCore, login_key: Option<String>) -> Result<()> {
    let viewport = ViewportBuilder::default().with_inner_size([1200.0, 800.0]);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let app = VellumGuiApp::new(
        app_core,
        login_key,
        INITIAL_LAYOUT_WIDTH as f32,
        INITIAL_LAYOUT_HEIGHT as f32,
    )?;

    eframe::run_native(
        "VellumFE GUI",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
    .map_err(|err| anyhow!("Failed to run GUI frontend: {}", err))
}

fn parse_hex_color(input: &str) -> Option<Color32> {
    let hex = input.strip_prefix('#').unwrap_or(input);
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::parse_hex_color;
    use eframe::egui::Color32;

    #[test]
    fn test_parse_hex_color_with_hash() {
        assert_eq!(
            parse_hex_color("#FF00AA"),
            Some(Color32::from_rgb(255, 0, 170))
        );
    }

    #[test]
    fn test_parse_hex_color_without_hash() {
        assert_eq!(
            parse_hex_color("00FF00"),
            Some(Color32::from_rgb(0, 255, 0))
        );
    }

    #[test]
    fn test_parse_hex_color_invalid_input() {
        assert_eq!(parse_hex_color("#XYZ"), None);
        assert_eq!(parse_hex_color(""), None);
    }
}
