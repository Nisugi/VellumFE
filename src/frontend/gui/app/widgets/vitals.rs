//! Progress-bar and countdown widgets: styled/skinned bars, vitals rows,
//! and RT/CT countdown rendering.

use super::*;

impl VellumGuiApp {
    /// is behind it.
    /// Paint the skin's `[controls.progressbar]` nine-slice over a bar that was
    /// just added, so its frame edges the fill. No-op without progress-bar art
    /// or without a response rect. Call right after `ui.add_sized(...)` for a
    /// styled_progress_bar.
    pub(super) fn overlay_progress_frame(
        ui: &egui::Ui,
        rect: egui::Rect,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) {
        if let Some(border) = skin_art.and_then(|art| art.control_border("progressbar", "normal")) {
            crate::frontend::gui::skin::paint_nine_slice(ui.painter(), rect, border, [true; 4]);
        }
    }

    pub(super) fn styled_progress_bar(
        ui: &egui::Ui,
        settings: &WidgetRenderSettings,
        fraction: f32,
        fill: Color32,
        text: String,
    ) -> egui::ProgressBar {
        // egui's ProgressBar clamps the fill to a minimum width of twice the
        // corner radius, which paints a colored sliver even at zero. Painting
        // the "fill" in the trough color hides it for genuinely empty bars.
        let fill = if fraction <= f32::EPSILON {
            ui.visuals().extreme_bg_color
        } else {
            fill
        };
        let mut bar = egui::ProgressBar::new(fraction)
            .fill(fill)
            .corner_radius(settings.bar_corner_radius);
        if !text.is_empty() {
            let behind = if fraction >= 0.5 {
                fill
            } else {
                ui.visuals().extreme_bg_color
            };
            let color = Self::readable_text_color(
                ui.visuals().text_color(),
                behind,
                settings.auto_contrast_bar_text,
            );
            bar = bar.text(RichText::new(text).color(color));
        }
        bar
    }

    pub(super) fn progress_fraction(value: u32, max: u32) -> f32 {
        if max == 0 {
            0.0
        } else {
            (value as f32 / max as f32).clamp(0.0, 1.0)
        }
    }

    // Visibility: tested from app/tests.rs.
    pub(in crate::frontend::gui::app) fn status_abbreviation(status: &str, target_cfg: &TargetListConfig) -> String {
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

    // Visibility: tested from app/tests.rs.
    pub(in crate::frontend::gui::app) fn normalize_entity_id(id: &str) -> String {
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

    /// Bar text for a vital with a true value/max pair (the core four).
    pub(super) fn vital_bar_text(
        format: crate::frontend::gui::persistence::VitalsTextFormat,
        label: &str,
        value: u32,
        max: u32,
        percent: u32,
        has_value_max: bool,
    ) -> String {
        use crate::frontend::gui::persistence::VitalsTextFormat as F;
        match format {
            F::LabelValueMax if has_value_max => format!("{}: {}/{}", label, value, max),
            F::LabelValueMax | F::LabelPercent => format!("{}: {}%", label, percent),
            F::ValueMax if has_value_max => format!("{}/{}", value, max),
            F::ValueMax | F::Percent => format!("{}%", percent),
            F::None => String::new(),
        }
    }

    /// Bar text for a percent-style vital that carries a status string
    /// ("clear as a bell", "None") instead of a value/max pair.
    pub(super) fn vital_status_text(
        format: crate::frontend::gui::persistence::VitalsTextFormat,
        label: &str,
        percent: u32,
        status: &str,
    ) -> String {
        use crate::frontend::gui::persistence::VitalsTextFormat as F;
        let status = status.trim();
        match format {
            F::LabelValueMax if !status.is_empty() => format!("{}: {}", label, status),
            F::LabelValueMax | F::LabelPercent => format!("{}: {}%", label, percent),
            F::ValueMax if !status.is_empty() => status.to_string(),
            F::ValueMax | F::Percent => format!("{}%", percent),
            F::None => String::new(),
        }
    }

    /// A standalone progress-bar window (stance, individual vital bars).
    /// Data arrives via dialog progressBar updates matched on `progress_id`.
    pub(super) fn render_single_progress_content(
        ui: &mut egui::Ui,
        data: &crate::data::ProgressData,
        settings: &WidgetRenderSettings,
    ) {
        let fraction = if data.max > 0 {
            Self::progress_fraction(data.value, data.max)
        } else {
            // Percent-style feeds (e.g. stance) report 0-100 with no max.
            (data.value.min(100) as f32) / 100.0
        };
        let fill = data
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| widget_accent(ui.ctx(), ui.visuals()));
        let text = if data.current_only {
            data.value.to_string()
        } else if data.numbers_only {
            format!("{}/{}", data.value, data.max)
        } else if data.label.is_empty() {
            format!("{}%", (fraction * 100.0).round() as u32)
        } else {
            data.label.clone()
        };
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let fraction = Self::animated_fraction(ui, &data.progress_id, fraction);
        let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
        let resp = ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
        Self::overlay_progress_frame(ui, resp.rect, settings.skin_art.as_deref());
    }

    pub(super) fn render_vitals_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
    ) {
        use crate::frontend::gui::persistence::{VitalKind, VitalsOrientation};

        let config = &settings.vitals;
        let minivitals = &app_core.game_state.minivitals;
        let fallback_vitals = &app_core.game_state.vitals;
        let has_full_vital_values = minivitals.health.max > 0
            || minivitals.mana.max > 0
            || minivitals.stamina.max > 0
            || minivitals.spirit.max > 0;

        let core_vital = |kind: VitalKind| -> (&'static str, u32, u32, u32, Color32) {
            match kind {
                VitalKind::Health => (
                    "Health",
                    minivitals.health.value,
                    minivitals.health.max,
                    fallback_vitals.health as u32,
                    Color32::from_rgb(0xcd, 0x4d, 0x4d),
                ),
                VitalKind::Mana => (
                    "Mana",
                    minivitals.mana.value,
                    minivitals.mana.max,
                    fallback_vitals.mana as u32,
                    Color32::from_rgb(0x47, 0x84, 0xd9),
                ),
                VitalKind::Stamina => (
                    "Stamina",
                    minivitals.stamina.value,
                    minivitals.stamina.max,
                    fallback_vitals.stamina as u32,
                    Color32::from_rgb(0x55, 0xb8, 0x6c),
                ),
                VitalKind::Spirit => (
                    "Spirit",
                    minivitals.spirit.value,
                    minivitals.spirit.max,
                    fallback_vitals.spirit as u32,
                    Color32::from_rgb(0xcb, 0xa9, 0x42),
                ),
                _ => unreachable!("core_vital called with a status vital"),
            }
        };

        // (animation id, fraction, bar text, fill color) per enabled bar.
        let bars: Vec<(&'static str, f32, String, Color32)> = config
            .bars
            .iter()
            .map(|kind| match kind {
                VitalKind::Health | VitalKind::Mana | VitalKind::Stamina | VitalKind::Spirit => {
                    let (label, value, max, fallback_pct, fill) = core_vital(*kind);
                    let (fraction, percent, usable_max) = if has_full_vital_values && max > 0 {
                        (
                            Self::progress_fraction(value, max),
                            (Self::progress_fraction(value, max) * 100.0).round() as u32,
                            true,
                        )
                    } else {
                        (fallback_pct.min(100) as f32 / 100.0, fallback_pct.min(100), false)
                    };
                    let text = Self::vital_bar_text(
                        config.text_format,
                        label,
                        value,
                        max,
                        percent,
                        usable_max,
                    );
                    (label, fraction, text, fill)
                }
                VitalKind::Mind => {
                    let exp = &app_core.game_state.gs4_experience;
                    let percent = exp.mind_state_value.min(100);
                    let text = Self::vital_status_text(
                        config.text_format,
                        "Mind",
                        percent,
                        &exp.mind_state_text,
                    );
                    (
                        "Mind",
                        percent as f32 / 100.0,
                        text,
                        Color32::from_rgb(0x7d, 0x8f, 0xb3),
                    )
                }
                VitalKind::Encumbrance => {
                    let encumbrance = &app_core.game_state.encumbrance;
                    let percent = encumbrance.value.min(100);
                    let text = Self::vital_status_text(
                        config.text_format,
                        "Encum",
                        percent,
                        &encumbrance.text,
                    );
                    (
                        "Encumbrance",
                        percent as f32 / 100.0,
                        text,
                        Color32::from_rgb(0xc0, 0x7f, 0x3f),
                    )
                }
                VitalKind::NextLevel => {
                    let exp = &app_core.game_state.gs4_experience;
                    let percent = exp.next_level_value.min(100);
                    let text = Self::vital_status_text(
                        config.text_format,
                        "Next",
                        percent,
                        &exp.next_level_text,
                    );
                    (
                        "Next Level",
                        percent as f32 / 100.0,
                        text,
                        Color32::from_rgb(0x3f, 0xa7, 0xa0),
                    )
                }
                VitalKind::Blood => {
                    let betrayer = &app_core.game_state.betrayer;
                    let percent = betrayer.value.min(100);
                    let text = Self::vital_bar_text(
                        config.text_format,
                        "Blood",
                        betrayer.value,
                        100,
                        percent,
                        false,
                    );
                    (
                        "Blood",
                        percent as f32 / 100.0,
                        text,
                        Color32::from_rgb(0x8a, 0x1f, 0x1f),
                    )
                }
            })
            .collect();

        if bars.is_empty() {
            ui.label("No vitals selected (right-click this window, Edit Window…).");
            return;
        }

        let bar_height = config.bar_height.clamp(8.0, 60.0);
        // egui's ProgressBar paints its trough with extreme_bg_color, so a
        // configured depleted color is applied by overriding the visuals of
        // the Ui the bars render into. styled_progress_bar also reads it for
        // empty-bar fill and text contrast, which keeps those consistent.
        let depleted_bg = config
            .depleted_color
            .as_deref()
            .and_then(super::theme::resolve_color);
        match config.orientation {
            VitalsOrientation::Horizontal => {
                ui.columns(bars.len(), |columns| {
                    for (column, (id, fraction, text, fill)) in
                        columns.iter_mut().zip(bars.into_iter())
                    {
                        if let Some(depleted) = depleted_bg {
                            column.visuals_mut().extreme_bg_color = depleted;
                        }
                        let fraction = Self::animated_fraction(column, id, fraction);
                        let bar = Self::styled_progress_bar(column, settings, fraction, fill, text);
                        let resp = column
                            .add_sized([column.available_width().max(40.0), bar_height], bar);
                        Self::overlay_progress_frame(column, resp.rect, settings.skin_art.as_deref());
                    }
                });
            }
            VitalsOrientation::Vertical => {
                if let Some(depleted) = depleted_bg {
                    ui.visuals_mut().extreme_bg_color = depleted;
                }
                for (id, fraction, text, fill) in bars {
                    let fraction = Self::animated_fraction(ui, id, fraction);
                    let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
                    let resp = ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
                    Self::overlay_progress_frame(ui, resp.rect, settings.skin_art.as_deref());
                }
            }
        }
    }

    /// Remaining whole seconds on a countdown, ceiling-rounded ("time until
    /// free"), adjusted for server clock drift.
    ///
    /// `local_unix_time_ms` carries millisecond precision so we don't add up
    /// to ~1s of floor bias on top of the server's whole-second RT/CT
    /// timestamp. Ceiling means a displayed "1" persists until the timer
    /// actually clears. Matches the TUI countdown widget exactly.
    pub(super) fn countdown_remaining_seconds(
        end_time: i64,
        server_time_offset: i64,
        local_unix_time_ms: i64,
    ) -> u32 {
        // end_time and server_time_offset are whole-second server-domain
        // values; lift into the millisecond domain to keep the math honest.
        let remaining_ms = end_time * 1000 - (local_unix_time_ms + server_time_offset * 1000);
        if remaining_ms <= 0 {
            0
        } else {
            ((remaining_ms + 999) / 1000) as u32 // integer ceiling
        }
    }

    /// Fractional remaining seconds on a countdown, so the drain bar moves a
    /// little on every repaint instead of stepping once per whole second.
    ///
    /// Uses the same offset convention as the whole-second helper —
    /// `end - (now + offset)` — so the bar fill and the displayed number
    /// agree on drifted clocks (previously the sign was flipped here).
    pub(super) fn countdown_remaining_seconds_f(
        end_time: i64,
        server_time_offset: i64,
        local_unix_time_f: f64,
    ) -> f32 {
        (end_time as f64 - (local_unix_time_f + server_time_offset as f64)).max(0.0) as f32
    }

    pub(super) fn render_countdown_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        countdown: &crate::data::CountdownData,
        settings: &WidgetRenderSettings,
    ) {
        let now_f = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or(0.0);
        let remaining = Self::countdown_remaining_seconds(
            countdown.end_time,
            app_core.server_time_offset,
            (now_f * 1000.0) as i64,
        );

        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let bar_width = ui.available_width().max(40.0);
        if remaining == 0 && !countdown.show_when_zero {
            // Idle timers render blank unless configured to stay visible.
            ui.allocate_space(Vec2::new(bar_width, bar_height));
            return;
        }

        // Bar is full at FULL_BAR_SECONDS or more and drains as the timer runs out.
        const FULL_BAR_SECONDS: u32 = 10;
        let remaining_f = Self::countdown_remaining_seconds_f(
            countdown.end_time,
            app_core.server_time_offset,
            now_f,
        );
        let fraction = remaining_f.min(FULL_BAR_SECONDS as f32) / FULL_BAR_SECONDS as f32;
        // Custom color override from the window config wins; otherwise the
        // fill falls back to the well-known per-timer defaults.
        let fill = countdown
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(
                || match countdown.countdown_id.to_ascii_lowercase().as_str() {
                    "roundtime" => Color32::from_rgb(0xcd, 0x4d, 0x4d),
                    "casttime" => Color32::from_rgb(0x47, 0x84, 0xd9),
                    _ => Color32::from_rgb(0xd9, 0x9a, 0x2b),
                },
            );
        let text = if countdown.label.is_empty() {
            format!("{remaining}")
        } else {
            format!("{}: {}", countdown.label, remaining)
        };
        let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
        ui.add_sized([bar_width, bar_height], bar);
    }

}
