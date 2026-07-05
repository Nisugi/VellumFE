//! Per-widget content renderers for the GUI.
//!
//! Pure-move extraction from `app.rs`: stateless associated helpers that
//! render `WindowContent` variants from `AppCore` state.

use super::*;

impl VellumGuiApp {
    pub(super) fn segment_to_rich_text(
        segment: &TextSegment,
        visuals: &egui::Visuals,
        is_link: bool,
    ) -> RichText {
        let foreground = segment
            .fg
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| {
                if is_link {
                    visuals.hyperlink_color
                } else {
                    visuals.text_color()
                }
            });
        let background = segment
            .bg
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(Color32::TRANSPARENT);

        let mut rich = RichText::new(segment.text.as_str())
            .size(DEFAULT_FONT_SIZE + if segment.bold { 0.5 } else { 0.0 })
            .color(foreground)
            .background_color(background);

        if segment.bold {
            rich = rich.strong();
        }
        if segment.mono {
            rich = rich.monospace();
        }
        rich
    }

    pub(super) fn segment_has_clickable_link(segment: &TextSegment) -> bool {
        // Parser may mark creature links as Monsterbold when links are wrapped in pushBold/popBold.
        // `link_data` is the reliable indicator of actual clickability.
        segment.link_data.is_some()
    }

    pub(super) fn render_styled_line(
        ui: &mut egui::Ui,
        line: &StyledLine,
        visuals: &egui::Visuals,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;

        ui.scope(|ui| {
            // Each styled segment is rendered as a separate widget. Keep inter-widget spacing at
            // zero so highlights/links don't introduce artificial spaces around punctuation.
            ui.spacing_mut().item_spacing.x = 0.0;

            ui.horizontal_wrapped(|ui| {
                for segment in &line.segments {
                    if segment.text.is_empty() {
                        continue;
                    }

                    let is_link = Self::segment_has_clickable_link(segment);
                    let rich = Self::segment_to_rich_text(segment, visuals, is_link);

                    if is_link {
                        let response = ui
                            .add(egui::Label::new(rich).sense(egui::Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() && clicked_link.is_none() {
                            if let Some(link_data) = segment.link_data.clone() {
                                let pointer_pos = response
                                    .interact_pointer_pos()
                                    .or_else(|| ui.ctx().pointer_latest_pos())
                                    .unwrap_or(Pos2::ZERO);
                                clicked_link = Some(GuiLinkClick {
                                    link_data,
                                    click_pos: Self::click_pos_to_grid(pointer_pos),
                                });
                            }
                        }
                    } else {
                        ui.label(rich);
                    }
                }
            });
        });

        clicked_link
    }

    pub(super) fn progress_fraction(value: u32, max: u32) -> f32 {
        if max == 0 {
            0.0
        } else {
            (value as f32 / max as f32).clamp(0.0, 1.0)
        }
    }

    pub(super) fn status_abbreviation(status: &str, target_cfg: &TargetListConfig) -> String {
        let status_lower = status.to_ascii_lowercase();
        target_cfg
            .status_abbrev
            .get(&status_lower)
            .cloned()
            .unwrap_or_else(|| {
                if status.chars().count() <= 3 {
                    status.to_string()
                } else {
                    status.chars().take(3).collect()
                }
            })
    }

    pub(super) fn normalize_entity_id(id: &str) -> String {
        id.trim().trim_start_matches('#').to_string()
    }

    pub(super) fn direct_command_link(command: String) -> LinkData {
        LinkData {
            exist_id: "_direct_".to_string(),
            noun: command,
            text: String::new(),
            coord: None,
        }
    }

    pub(super) fn gui_link_click_from_response(
        response: &egui::Response,
        ui: &egui::Ui,
        link_data: LinkData,
    ) -> GuiLinkClick {
        let pointer_pos = response
            .interact_pointer_pos()
            .or_else(|| ui.ctx().pointer_latest_pos())
            .unwrap_or(Pos2::ZERO);
        GuiLinkClick {
            link_data,
            click_pos: Self::click_pos_to_grid(pointer_pos),
        }
    }

    pub(super) fn render_vitals_content(app_core: &AppCore, ui: &mut egui::Ui) {
        let minivitals = &app_core.game_state.minivitals;
        let fallback_vitals = &app_core.game_state.vitals;
        let has_full_vital_values = minivitals.health.max > 0
            || minivitals.mana.max > 0
            || minivitals.stamina.max > 0
            || minivitals.spirit.max > 0;

        let bars = [
            (
                "Health",
                minivitals.health.value,
                minivitals.health.max,
                fallback_vitals.health as u32,
                Color32::from_rgb(0xcd, 0x4d, 0x4d),
            ),
            (
                "Mana",
                minivitals.mana.value,
                minivitals.mana.max,
                fallback_vitals.mana as u32,
                Color32::from_rgb(0x47, 0x84, 0xd9),
            ),
            (
                "Stamina",
                minivitals.stamina.value,
                minivitals.stamina.max,
                fallback_vitals.stamina as u32,
                Color32::from_rgb(0x55, 0xb8, 0x6c),
            ),
            (
                "Spirit",
                minivitals.spirit.value,
                minivitals.spirit.max,
                fallback_vitals.spirit as u32,
                Color32::from_rgb(0xcb, 0xa9, 0x42),
            ),
        ];

        let bar_height = ui.spacing().interact_size.y.max(16.0);

        ui.columns(4, |columns| {
            for (column, (label, value, max, fallback_pct, fill_color)) in
                columns.iter_mut().zip(bars.into_iter())
            {
                let (fraction, text) = if has_full_vital_values && max > 0 {
                    (
                        Self::progress_fraction(value, max),
                        format!("{}: {}/{}", label, value, max),
                    )
                } else {
                    let clamped_pct = fallback_pct.min(100);
                    (
                        clamped_pct as f32 / 100.0,
                        format!("{}: {}%", label, clamped_pct),
                    )
                };
                column.add_sized(
                    [column.available_width().max(40.0), bar_height],
                    egui::ProgressBar::new(fraction)
                        .text(text)
                        .fill(fill_color),
                );
            }
        });
    }

    /// Remaining whole seconds on a countdown, adjusted for server clock drift.
    pub(super) fn countdown_remaining_seconds(
        end_time: i64,
        server_time_offset: i64,
        local_unix_time: i64,
    ) -> u32 {
        (end_time - (local_unix_time + server_time_offset)).max(0) as u32
    }

    pub(super) fn render_countdown_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        countdown: &crate::data::CountdownData,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0);
        let remaining =
            Self::countdown_remaining_seconds(countdown.end_time, app_core.server_time_offset, now);

        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let bar_width = ui.available_width().max(40.0);
        if remaining == 0 {
            // Idle timers render blank, matching the TUI.
            ui.allocate_space(Vec2::new(bar_width, bar_height));
            return;
        }

        // Bar is full at FULL_BAR_SECONDS or more and drains as the timer runs out.
        const FULL_BAR_SECONDS: u32 = 10;
        let fraction = remaining.min(FULL_BAR_SECONDS) as f32 / FULL_BAR_SECONDS as f32;
        let fill = match countdown.countdown_id.to_ascii_lowercase().as_str() {
            "roundtime" => Color32::from_rgb(0xcd, 0x4d, 0x4d),
            "casttime" => Color32::from_rgb(0x47, 0x84, 0xd9),
            _ => Color32::from_rgb(0xd9, 0x9a, 0x2b),
        };
        let text = if countdown.label.is_empty() {
            format!("{remaining}")
        } else {
            format!("{}: {}", countdown.label, remaining)
        };
        ui.add_sized(
            [bar_width, bar_height],
            egui::ProgressBar::new(fraction).text(text).fill(fill),
        );
    }

    pub(super) fn render_compass_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        compass_data: &crate::data::CompassData,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;
        let source_directions: &[String] = if compass_data.directions.is_empty() {
            &app_core.game_state.compass_dirs
        } else {
            &compass_data.directions
        };
        let available: HashSet<String> = source_directions
            .iter()
            .map(|direction| direction.to_ascii_lowercase())
            .collect();

        let grid_rows: [[&str; 3]; 3] = [["nw", "n", "ne"], ["w", "", "e"], ["sw", "s", "se"]];
        egui::Grid::new("gui_compass_grid")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for row in grid_rows {
                    for direction in row {
                        if direction.is_empty() {
                            ui.label("");
                            continue;
                        }
                        let is_available = available.contains(direction);
                        let label = direction.to_ascii_uppercase();
                        let response = ui.add_enabled(
                            is_available,
                            egui::Button::new(label).min_size(Vec2::splat(26.0)),
                        );
                        if is_available && response.clicked() && clicked_link.is_none() {
                            clicked_link = Some(Self::gui_link_click_from_response(
                                &response,
                                ui,
                                Self::direct_command_link(direction.to_string()),
                            ));
                        }
                    }
                    ui.end_row();
                }
            });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for direction in ["up", "down", "out", "in"] {
                let is_available = available.contains(direction);
                let label = direction.to_ascii_uppercase();
                let response = ui.add_enabled(is_available, egui::Button::new(label));
                if is_available && response.clicked() && clicked_link.is_none() {
                    clicked_link = Some(Self::gui_link_click_from_response(
                        &response,
                        ui,
                        Self::direct_command_link(direction.to_string()),
                    ));
                }
            }
        });

        clicked_link
    }

    pub(super) fn render_hand_content(
        ui: &mut egui::Ui,
        hand_prefix: &str,
        item: &Option<String>,
        _link: &Option<LinkData>,
    ) -> Option<GuiLinkClick> {
        let empty_text = if hand_prefix == "S" { "None" } else { "Empty" };
        let item_text = item
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or(empty_text);
        let icon_text = match hand_prefix {
            "L" => "[L]",
            "R" => "[R]",
            "S" => "[S]",
            _ => "[?]",
        };
        // Keep hand rows compact and content-sized so they don't request full window width.
        let display_text = if item_text.chars().count() > 56 {
            let mut truncated: String = item_text.chars().take(53).collect();
            truncated.push_str("...");
            truncated
        } else {
            item_text.to_string()
        };
        let row_height = ui.spacing().interact_size.y.max(16.0);
        let icon_width = 22.0;
        let icon_gap = 4.0;
        let handle_gutter_width = 12.0;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_sized(
                [icon_width, row_height],
                egui::Label::new(RichText::new(icon_text).monospace().strong()),
            );
            ui.add_space(icon_gap);
            let text_width = (ui.available_width() - handle_gutter_width).max(1.0);
            ui.add_sized([text_width, row_height], egui::Label::new(display_text).truncate());
            ui.add_space(handle_gutter_width);
        });

        None
    }

    pub(super) fn render_room_entities(ui: &mut egui::Ui, label: &str, values: &[String]) {
        if values.is_empty() {
            return;
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{}:", label)).strong());
            ui.label(values.join(", "));
        });
    }

    pub(super) fn render_room_exits(ui: &mut egui::Ui, exits: &[String]) -> Option<GuiLinkClick> {
        if exits.is_empty() {
            return None;
        }

        let mut clicked_link = None;
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Exits:").strong());
            for (index, exit) in exits.iter().enumerate() {
                let response = ui
                    .add(egui::Label::new(exit.as_str()).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() && clicked_link.is_none() {
                    clicked_link = Some(Self::gui_link_click_from_response(
                        &response,
                        ui,
                        Self::direct_command_link(exit.to_string()),
                    ));
                }
                if index + 1 < exits.len() {
                    ui.label(",");
                }
            }
        });
        clicked_link
    }

    pub(super) fn render_active_effects_content(
        ui: &mut egui::Ui,
        effects_content: &crate::data::ActiveEffectsContent,
    ) {
        if effects_content.effects.is_empty() {
            ui.label(format!("No active {}.", effects_content.category));
            return;
        }

        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt(format!("active_effects_{}", effects_content.category))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for effect in &effects_content.effects {
                    ui.horizontal(|ui| {
                        let mut text = RichText::new(effect.text.as_str());
                        if let Some(color) = effect.text_color.as_deref().and_then(parse_hex_color)
                        {
                            text = text.color(color);
                        }
                        ui.label(text);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !effect.time.trim().is_empty() {
                                ui.label(RichText::new(effect.time.as_str()).small().weak());
                            }
                        });
                    });

                    let mut bar = egui::ProgressBar::new((effect.value.min(100) as f32) / 100.0)
                        .text(format!("{}%", effect.value.min(100)));
                    if let Some(fill) = effect.bar_color.as_deref().and_then(parse_hex_color) {
                        bar = bar.fill(fill);
                    }
                    ui.add(bar);
                    ui.add_space(4.0);
                }
            });
    }

    pub(super) fn format_target_line(
        creature: &crate::core::state::Creature,
        target_cfg: &TargetListConfig,
    ) -> String {
        let status_tag = creature
            .status
            .as_deref()
            .map(|status| format!("[{}]", Self::status_abbreviation(status, target_cfg)));
        if let Some(status) = status_tag {
            if target_cfg.status_position.eq_ignore_ascii_case("start") {
                format!("{} {}", status, creature.name)
            } else {
                format!("{} {}", creature.name, status)
            }
        } else {
            creature.name.clone()
        }
    }

    pub(super) fn format_player_line(
        player: &crate::core::state::Player,
        target_cfg: &TargetListConfig,
    ) -> String {
        let mut statuses = Vec::new();
        if let Some(primary) = player.primary_status.as_deref() {
            statuses.push(format!(
                "[{}]",
                Self::status_abbreviation(primary, target_cfg)
            ));
        }
        if let Some(secondary) = player.secondary_status.as_deref() {
            statuses.push(format!(
                "[{}]",
                Self::status_abbreviation(secondary, target_cfg)
            ));
        }

        if statuses.is_empty() {
            return player.name.clone();
        }

        if target_cfg.status_position.eq_ignore_ascii_case("start") {
            format!("{} {}", statuses.join(" "), player.name)
        } else {
            format!("{} {}", player.name, statuses.join(" "))
        }
    }

    pub(super) fn render_targets_content(app_core: &AppCore, ui: &mut egui::Ui) -> Option<GuiLinkClick> {
        let mut clicked_link = None;
        let target_cfg = &app_core.config.target_list;
        let current_target =
            Self::normalize_entity_id(&app_core.game_state.target_list.current_target);
        let targetable_ids: HashSet<String> = app_core
            .game_state
            .target_list
            .target_ids
            .iter()
            .map(|id| Self::normalize_entity_id(id))
            .collect();

        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("targets_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for creature in &app_core.game_state.room_creatures {
                    let creature_id = Self::normalize_entity_id(&creature.id);
                    if !targetable_ids.is_empty() && !targetable_ids.contains(&creature_id) {
                        continue;
                    }
                    if Self::should_filter_target_creature(creature, target_cfg) {
                        continue;
                    }

                    let display_text = Self::format_target_line(creature, target_cfg);
                    let is_current = !current_target.is_empty() && creature_id == current_target;
                    let styled = if is_current {
                        RichText::new(format!("> {}", display_text))
                            .color(Color32::from_rgb(0x62, 0xcf, 0x79))
                    } else {
                        RichText::new(display_text)
                    };
                    let response = ui
                        .add(egui::Label::new(styled).sense(egui::Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() && clicked_link.is_none() {
                        clicked_link = Some(Self::gui_link_click_from_response(
                            &response,
                            ui,
                            Self::direct_command_link(format!("target #{}", creature_id)),
                        ));
                    }
                }
            });

        clicked_link
    }

    pub(super) fn should_filter_target_creature(
        creature: &crate::core::state::Creature,
        target_cfg: &TargetListConfig,
    ) -> bool {
        if let Some(status) = creature.status.as_deref() {
            let status_lower = status.to_ascii_lowercase();
            if status_lower.contains("dead") || status_lower.contains("gone") {
                return true;
            }
        }

        let name_lower = creature.name.to_ascii_lowercase();
        if name_lower.starts_with("animated") && !name_lower.starts_with("animated slush") {
            return true;
        }

        creature
            .noun
            .as_ref()
            .map(|noun| noun.to_ascii_lowercase())
            .is_some_and(|noun| {
                target_cfg
                    .excluded_nouns
                    .iter()
                    .any(|excluded| excluded == &noun)
            })
    }

    pub(super) fn render_players_content(app_core: &AppCore, ui: &mut egui::Ui) -> Option<GuiLinkClick> {
        let mut clicked_link = None;
        let target_cfg = &app_core.config.target_list;

        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("players_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for player in &app_core.game_state.room_players {
                    let display_text = Self::format_player_line(player, target_cfg);
                    let response = ui
                        .add(egui::Label::new(display_text).sense(egui::Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand);

                    if response.clicked() && clicked_link.is_none() {
                        let link_data = LinkData {
                            exist_id: player.id.clone(),
                            noun: player.name.clone(),
                            text: player.name.clone(),
                            coord: None,
                        };
                        clicked_link =
                            Some(Self::gui_link_click_from_response(&response, ui, link_data));
                    }
                }
            });

        clicked_link
    }

    pub(super) fn render_text_content(
        ui: &mut egui::Ui,
        content: &TextContent,
        scroll_id: &str,
    ) -> Option<GuiLinkClick> {
        let visuals = ui.visuals().clone();
        let mut clicked_link = None;
        let start = content.lines.len().saturating_sub(MAX_RENDERED_LINES);
        let max_height = ui.available_height().max(1.0);

        egui::ScrollArea::vertical()
            .id_salt(format!("text_scroll_{}", scroll_id))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for line in content.lines.iter().skip(start) {
                    if let Some(link) = Self::render_styled_line(ui, line, &visuals) {
                        clicked_link = Some(link);
                    }
                }
            });
        clicked_link
    }

    pub(super) fn render_room_description(
        ui: &mut egui::Ui,
        lines: &[StyledLine],
        scroll_id: &str,
    ) -> Option<GuiLinkClick> {
        let visuals = ui.visuals().clone();
        let mut clicked_link = None;
        let max_height = ui.available_height().max(1.0);

        egui::ScrollArea::vertical()
            .id_salt(format!("room_scroll_{}", scroll_id))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for line in lines {
                    if let Some(link) = Self::render_styled_line(ui, line, &visuals) {
                        clicked_link = Some(link);
                    }
                }
            });

        clicked_link
    }

    pub(super) fn render_window_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        tab: &GuiTab,
    ) -> Option<GuiLinkClick> {
        let Some(window) = app_core.ui_state.windows.get(&tab.window_name) else {
            ui.label("This tab's source window is no longer available.");
            return None;
        };

        match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Spells(content) => {
                Self::render_text_content(ui, content, &tab.window_name)
            }
            WindowContent::Progress(_) | WindowContent::MiniVitals => {
                Self::render_vitals_content(app_core, ui);
                None
            }
            WindowContent::Compass(compass) => Self::render_compass_content(app_core, ui, compass),
            WindowContent::Hand { item, link } => {
                let hand_prefix = if window.name.to_ascii_lowercase().contains("left") {
                    "L"
                } else if window.name.to_ascii_lowercase().contains("right") {
                    "R"
                } else {
                    "S"
                };
                Self::render_hand_content(ui, hand_prefix, item, link)
            }
            WindowContent::TabbedText(tabbed) => {
                if let Some(active) = tabbed.tabs.get(tabbed.active_tab_index) {
                    Self::render_text_content(ui, &active.content, &tab.window_name)
                } else {
                    ui.label("No active tab content.");
                    None
                }
            }
            WindowContent::Room(room) => {
                ui.heading(&room.name);
                ui.separator();
                let mut clicked_link =
                    Self::render_room_description(ui, &room.description, &tab.window_name);
                if let Some(exit_click) = Self::render_room_exits(ui, &room.exits) {
                    if clicked_link.is_none() {
                        clicked_link = Some(exit_click);
                    }
                }
                Self::render_room_entities(ui, "Players", &room.players);
                Self::render_room_entities(ui, "Objects", &room.objects);
                clicked_link
            }
            WindowContent::ActiveEffects(content) => {
                Self::render_active_effects_content(ui, content);
                None
            }
            WindowContent::Targets => Self::render_targets_content(app_core, ui),
            WindowContent::Players => Self::render_players_content(app_core, ui),
            WindowContent::Countdown(countdown) => {
                Self::render_countdown_content(app_core, ui, countdown);
                None
            }
            _ => {
                ui.label("Widget rendering for this tab is scheduled for later GUI milestones.");
                ui.label(format!(
                    "Window: {} ({:?})",
                    window.name, window.widget_type
                ));
                None
            }
        }
    }
}

pub(super) fn parse_hex_color(input: &str) -> Option<Color32> {
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
    use super::VellumGuiApp;

    #[test]
    fn countdown_remaining_clamps_to_zero_when_elapsed() {
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(100, 0, 150), 0);
    }

    #[test]
    fn countdown_remaining_counts_down_from_end_time() {
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 100), 10);
    }

    #[test]
    fn countdown_remaining_applies_server_offset() {
        // Server clock runs 5s ahead of local time.
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 5, 100), 5);
    }
}
