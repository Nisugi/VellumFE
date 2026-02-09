use super::persistence::{load_layout, save_layout, GuiLayoutFileV1};
use super::{TabId, TabKey};
use crate::config::Config;
use crate::core::AppCore;
use crate::data::{
    SpanType, StyledLine, TabbedTextContent, TextContent, WidgetType, WindowContent, WindowState,
};
use crate::network::{LichConnection, RawLogger, ServerMessage};
use anyhow::{anyhow, Context, Result};
use eframe::egui;
use eframe::egui::text::LayoutJob;
use eframe::egui::{Color32, FontFamily, FontId, RichText, TextFormat, ViewportBuilder};
use egui_dock::{DockArea, DockState, TabViewer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::mpsc;

const INITIAL_LAYOUT_WIDTH: u16 = 160;
const INITIAL_LAYOUT_HEIGHT: u16 = 50;
const MAX_RENDERED_LINES: usize = 2000;
const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(Clone, Debug)]
struct GuiTab {
    id: TabId,
    window_name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DockStateSnapshot {
    visible_tabs: Vec<TabKey>,
}

pub struct VellumGuiApp {
    app_core: AppCore,
    _runtime: tokio::runtime::Runtime,
    command_tx: mpsc::UnboundedSender<String>,
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    network_handle: Option<tokio::task::JoinHandle<()>>,
    command_input: String,
    close_requested: bool,
    dock_state: Option<DockState<GuiTab>>,
    available_tabs: HashMap<TabKey, GuiTab>,
    hidden_tabs: HashSet<TabKey>,
    layout_profile: String,
    layout_character: String,
    layout_dirty: bool,
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

        let (layout_profile, layout_character) = Self::resolve_layout_ids(&app_core.config);
        let persisted_layout = load_layout(&layout_profile, &layout_character).ok();

        let available_tabs = Self::collect_available_tabs(&app_core);
        let mut hidden_tabs: HashSet<TabKey> = persisted_layout
            .as_ref()
            .map(|layout| layout.hidden_tabs.iter().cloned().collect())
            .unwrap_or_default();
        hidden_tabs.retain(|key| available_tabs.contains_key(key));

        let snapshot = persisted_layout
            .as_ref()
            .and_then(|layout| Self::dock_snapshot_from_layout(layout));

        let visible_tabs =
            Self::build_visible_tabs(&available_tabs, &hidden_tabs, snapshot.as_ref());
        let dock_state = if visible_tabs.is_empty() {
            None
        } else {
            Some(DockState::new(visible_tabs))
        };

        Ok(Self {
            app_core,
            _runtime: runtime,
            command_tx,
            server_rx,
            network_handle: Some(network_handle),
            command_input: String::new(),
            close_requested: false,
            dock_state,
            available_tabs,
            hidden_tabs,
            layout_profile,
            layout_character,
            layout_dirty: false,
        })
    }

    fn resolve_layout_ids(config: &Config) -> (String, String) {
        let profile_id = config
            .character
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let character_id = config
            .connection
            .character
            .clone()
            .or_else(|| config.character.clone())
            .unwrap_or_else(|| "default".to_string());
        (profile_id, character_id)
    }

    fn collect_available_tabs(app_core: &AppCore) -> HashMap<TabKey, GuiTab> {
        let mut keys: Vec<String> = app_core.ui_state.windows.keys().cloned().collect();
        keys.sort();

        let mut tabs = HashMap::new();
        for name in keys {
            let Some(window) = app_core.ui_state.windows.get(&name) else {
                continue;
            };

            let Some(tab_key) = Self::tab_key_for_window(&name, window) else {
                continue;
            };

            tabs.entry(tab_key.clone()).or_insert_with(|| GuiTab {
                id: TabId::with_title(tab_key, window.name.clone()),
                window_name: name.clone(),
            });
        }

        tabs
    }

    fn tab_key_for_window(name: &str, window: &WindowState) -> Option<TabKey> {
        let key = match window.widget_type {
            WidgetType::CommandInput | WidgetType::Spacer => return None,
            WidgetType::Text | WidgetType::TabbedText => {
                if Self::is_main_stream_window(name, window) {
                    TabKey::TextMain
                } else {
                    TabKey::TextByName {
                        id: name.to_string(),
                    }
                }
            }
            WidgetType::Inventory => TabKey::Inventory {
                id: name.to_string(),
            },
            WidgetType::ActiveEffects => TabKey::ActiveEffects {
                id: name.to_string(),
            },
            WidgetType::Quickbar => TabKey::Quickbar {
                id: name.to_string(),
            },
            WidgetType::MiniVitals | WidgetType::Progress => TabKey::Vitals,
            WidgetType::Countdown => TabKey::Countdown,
            WidgetType::Compass => TabKey::Compass,
            WidgetType::Indicator => TabKey::Indicators,
            WidgetType::Targets => TabKey::Targets,
            WidgetType::Players => TabKey::Players,
            WidgetType::Room => TabKey::Room,
            WidgetType::Experience | WidgetType::GS4Experience => TabKey::Experience,
            WidgetType::InjuryDoll => TabKey::InjuryDoll,
            WidgetType::Dashboard => TabKey::Dashboard,
            WidgetType::Encumbrance => TabKey::Encumbrance,
            WidgetType::Perception => TabKey::Perception,
            WidgetType::Hand => {
                let lower = name.to_ascii_lowercase();
                if lower.contains("left") {
                    TabKey::LeftHand
                } else if lower.contains("right") {
                    TabKey::RightHand
                } else {
                    TabKey::SpellHand
                }
            }
            _ => TabKey::TextByName {
                id: name.to_string(),
            },
        };

        Some(key)
    }

    fn is_main_stream_window(name: &str, window: &WindowState) -> bool {
        if name.eq_ignore_ascii_case("main") {
            return true;
        }

        match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Spells(content) => content
                .streams
                .iter()
                .any(|stream| stream.eq_ignore_ascii_case("main")),
            WindowContent::TabbedText(tabbed) => Self::find_main_tab(tabbed).is_some(),
            _ => false,
        }
    }

    fn find_main_tab(tabbed: &TabbedTextContent) -> Option<&crate::data::TabState> {
        tabbed.tabs.iter().find(|tab| {
            tab.definition
                .streams
                .iter()
                .any(|stream| stream.eq_ignore_ascii_case("main"))
        })
    }

    fn dock_snapshot_from_layout(layout: &GuiLayoutFileV1) -> Option<DockStateSnapshot> {
        if layout.dock_state_json.is_null() {
            return None;
        }
        serde_json::from_value(layout.dock_state_json.clone()).ok()
    }

    fn build_visible_tabs(
        available_tabs: &HashMap<TabKey, GuiTab>,
        hidden_tabs: &HashSet<TabKey>,
        snapshot: Option<&DockStateSnapshot>,
    ) -> Vec<GuiTab> {
        let mut visible = Vec::new();
        let mut seen = HashSet::new();

        if let Some(snapshot) = snapshot {
            for key in &snapshot.visible_tabs {
                if hidden_tabs.contains(key) {
                    continue;
                }
                if let Some(tab) = available_tabs.get(key) {
                    visible.push(tab.clone());
                    seen.insert(key.clone());
                }
            }
        }

        let mut fallback_tabs: Vec<GuiTab> = available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if hidden_tabs.contains(key) || seen.contains(key) {
                    None
                } else {
                    Some(tab.clone())
                }
            })
            .collect();
        fallback_tabs.sort_by_key(|tab| tab.id.title.to_ascii_lowercase());

        visible.extend(fallback_tabs);
        visible
    }

    fn default_visible_tab_keys(&self) -> Vec<TabKey> {
        let mut visible: Vec<(String, TabKey)> = self
            .available_tabs
            .iter()
            .filter_map(|(key, tab)| {
                if self.hidden_tabs.contains(key) {
                    None
                } else {
                    Some((tab.id.title.clone(), key.clone()))
                }
            })
            .collect();
        visible.sort_by_key(|(title, _)| title.to_ascii_lowercase());
        visible.into_iter().map(|(_, key)| key).collect()
    }

    fn rebuild_dock_state(&mut self) {
        let visible_tabs = Self::build_visible_tabs(&self.available_tabs, &self.hidden_tabs, None);
        self.dock_state = if visible_tabs.is_empty() {
            None
        } else {
            Some(DockState::new(visible_tabs))
        };
    }

    fn refresh_available_tabs_if_needed(&mut self) {
        let refreshed = Self::collect_available_tabs(&self.app_core);
        if refreshed.len() == self.available_tabs.len()
            && refreshed
                .keys()
                .all(|key| self.available_tabs.contains_key(key))
        {
            return;
        }

        self.available_tabs = refreshed;
        self.hidden_tabs
            .retain(|key| self.available_tabs.contains_key(key));
        self.rebuild_dock_state();
        self.layout_dirty = true;
    }

    fn hide_tab(&mut self, key: TabKey) {
        if self.hidden_tabs.insert(key) {
            self.rebuild_dock_state();
            self.layout_dirty = true;
        }
    }

    fn restore_tab(&mut self, key: TabKey) {
        if self.hidden_tabs.remove(&key) {
            self.rebuild_dock_state();
            self.layout_dirty = true;
        }
    }

    fn hidden_tabs_for_menu(&self) -> Vec<(TabKey, String)> {
        let mut entries: Vec<(TabKey, String)> = self
            .hidden_tabs
            .iter()
            .filter_map(|key| {
                self.available_tabs
                    .get(key)
                    .map(|tab| (key.clone(), tab.id.title.clone()))
            })
            .collect();
        entries.sort_by_key(|(_, title)| title.to_ascii_lowercase());
        entries
    }

    fn save_layout_state(&mut self) {
        let mut layout = GuiLayoutFileV1::new(&self.layout_profile, &self.layout_character);

        let mut hidden_tabs: Vec<TabKey> = self.hidden_tabs.iter().cloned().collect();
        hidden_tabs.sort_by_key(|key| key.short_id());
        layout.hidden_tabs = hidden_tabs;

        let snapshot = DockStateSnapshot {
            visible_tabs: self.default_visible_tab_keys(),
        };
        layout.dock_state_json = serde_json::to_value(snapshot).unwrap_or(serde_json::Value::Null);

        if let Err(err) = save_layout(&layout, &self.layout_profile, &self.layout_character) {
            tracing::warn!("Failed to save GUI layout: {}", err);
        }
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

    fn render_styled_lines(ui: &mut egui::Ui, lines: &std::collections::VecDeque<StyledLine>) {
        let visuals = ui.visuals().clone();
        let start = lines.len().saturating_sub(MAX_RENDERED_LINES);
        for line in lines.iter().skip(start) {
            let job = Self::line_to_layout_job(line, &visuals);
            ui.label(job);
        }
    }

    fn render_text_content(ui: &mut egui::Ui, content: &TextContent, scroll_id: &str) {
        egui::ScrollArea::vertical()
            .id_salt(format!("text_scroll_{}", scroll_id))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Self::render_styled_lines(ui, &content.lines);
            });
    }

    fn render_window_content(app_core: &AppCore, ui: &mut egui::Ui, tab: &GuiTab) {
        let Some(window) = app_core.ui_state.windows.get(&tab.window_name) else {
            ui.label("This tab's source window is no longer available.");
            return;
        };

        match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Spells(content) => {
                Self::render_text_content(ui, content, &tab.window_name)
            }
            WindowContent::TabbedText(tabbed) => {
                if let Some(active) = tabbed.tabs.get(tabbed.active_tab_index) {
                    ui.label(
                        RichText::new(format!("Active tab: {}", active.definition.name)).italics(),
                    );
                    ui.separator();
                    Self::render_text_content(ui, &active.content, &tab.window_name);
                } else {
                    ui.label("No active tab content.");
                }
            }
            WindowContent::Room(room) => {
                ui.heading(&room.name);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt(format!("room_scroll_{}", tab.window_name))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Self::render_styled_lines(
                            ui,
                            &std::collections::VecDeque::from(room.description.clone()),
                        );
                        if !room.exits.is_empty() {
                            ui.separator();
                            ui.label(format!("Exits: {}", room.exits.join(", ")));
                        }
                    });
            }
            _ => {
                ui.label("Widget rendering for this tab is scheduled for later GUI milestones.");
                ui.label(format!(
                    "Window: {} ({:?})",
                    window.name, window.widget_type
                ));
            }
        }
    }
}

struct GuiDockTabViewer<'a> {
    app_core: &'a AppCore,
    closed_tabs: Vec<TabKey>,
}

impl<'a> GuiDockTabViewer<'a> {
    fn new(app_core: &'a AppCore) -> Self {
        Self {
            app_core,
            closed_tabs: Vec::new(),
        }
    }

    fn take_closed_tabs(self) -> Vec<TabKey> {
        self.closed_tabs
    }
}

impl TabViewer for GuiDockTabViewer<'_> {
    type Tab = GuiTab;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        VellumGuiApp::render_window_content(self.app_core, ui, tab);
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.id.title.clone().into()
    }

    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        true
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        self.closed_tabs.push(tab.id.key.clone());
        true
    }
}

impl eframe::App for VellumGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_server_messages();
        self.refresh_available_tabs_if_needed();

        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let mut restore_key: Option<TabKey> = None;

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
                ui.separator();

                ui.menu_button("Hidden Tabs", |ui| {
                    let hidden = self.hidden_tabs_for_menu();
                    if hidden.is_empty() {
                        ui.label("No hidden tabs");
                    } else {
                        for (key, title) in hidden {
                            if ui.button(title).clicked() {
                                restore_key = Some(key);
                                ui.close_menu();
                            }
                        }
                    }
                });
            });
        });

        if let Some(key) = restore_key {
            self.restore_tab(key);
        }

        let mut closed_tabs = Vec::new();
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(dock_state) = &mut self.dock_state {
                let mut viewer = GuiDockTabViewer::new(&self.app_core);
                DockArea::new(dock_state).show_inside(ui, &mut viewer);
                closed_tabs = viewer.take_closed_tabs();
            } else {
                ui.heading("No visible tabs");
                ui.label("Use Hidden Tabs to restore one or more tabs.");
            }
        });

        for key in closed_tabs {
            self.hide_tab(key);
        }

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

        if self.layout_dirty {
            self.save_layout_state();
            self.layout_dirty = false;
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl Drop for VellumGuiApp {
    fn drop(&mut self) {
        if let Some(handle) = self.network_handle.take() {
            handle.abort();
        }

        if self.layout_dirty {
            self.save_layout_state();
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
    use super::{parse_hex_color, DockStateSnapshot, VellumGuiApp};
    use crate::config::Config;
    use crate::frontend::gui::TabKey;
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

    #[test]
    fn test_resolve_layout_ids_prefers_connection_character() {
        let mut config = Config::default();
        config.character = Some("profile_a".to_string());
        config.connection.character = Some("Nisugi".to_string());

        let (profile, character) = VellumGuiApp::resolve_layout_ids(&config);
        assert_eq!(profile, "profile_a");
        assert_eq!(character, "Nisugi");
    }

    #[test]
    fn test_dock_state_snapshot_round_trip() {
        let snapshot = DockStateSnapshot {
            visible_tabs: vec![TabKey::TextMain, TabKey::Vitals],
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: DockStateSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.visible_tabs.len(), 2);
        assert_eq!(parsed.visible_tabs[0], TabKey::TextMain);
        assert_eq!(parsed.visible_tabs[1], TabKey::Vitals);
    }
}
