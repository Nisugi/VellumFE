//! Per-widget content renderers for the GUI.
//!
//! Pure-move extraction from `app.rs`: stateless associated helpers that
//! render `WindowContent` variants from `AppCore` state.

use super::*;

/// Seconds for a value-driven bar to glide to a new target value.
const BAR_ANIMATION_SECONDS: f32 = 0.2;

impl VellumGuiApp {
    /// Animate a bar fraction toward its target so server updates glide
    /// instead of jumping. The first paint for a given id snaps straight to
    /// the target, and egui keeps repainting while the value is moving, so
    /// this composes with repaint-on-demand at zero idle cost.
    fn animated_fraction(ui: &egui::Ui, id_salt: &str, target: f32) -> f32 {
        ui.ctx()
            .animate_value_with_time(ui.id().with(id_salt), target, BAR_ANIMATION_SECONDS)
    }

    pub(super) fn segment_to_rich_text(
        segment: &TextSegment,
        visuals: &egui::Visuals,
        is_link: bool,
        search_match: bool,
        font_id: &egui::FontId,
    ) -> RichText {
        Self::styled_rich_text(&segment.text, segment, visuals, is_link, search_match, font_id)
    }

    /// Build rich text with a segment's styling for an arbitrary slice of its
    /// text (used to highlight exact search-match runs within a segment).
    fn styled_rich_text(
        text: &str,
        segment: &TextSegment,
        visuals: &egui::Visuals,
        is_link: bool,
        search_match: bool,
        font_id: &egui::FontId,
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
        let background = if search_match {
            visuals.selection.bg_fill
        } else {
            segment
                .bg
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(Color32::TRANSPARENT)
        };

        let mut rich = RichText::new(text)
            .font(egui::FontId {
                size: font_id.size + if segment.bold { 0.5 } else { 0.0 },
                family: font_id.family.clone(),
            })
            .color(foreground)
            .background_color(background);

        if segment.bold {
            rich = rich.strong();
        }
        if segment.mono {
            // Overrides the family only; the size above is kept.
            rich = rich.monospace();
        }
        rich
    }

    pub(super) fn segment_has_clickable_link(segment: &TextSegment) -> bool {
        // Parser may mark creature links as Monsterbold when links are wrapped in pushBold/popBold.
        // `link_data` is the reliable indicator of actual clickability.
        segment.link_data.is_some()
    }

    /// Allocation-free ASCII case-insensitive substring search starting at
    /// `from`. `needle_lower` must already be ASCII-lowercased. Byte indices
    /// returned are always char boundaries: a valid UTF-8 needle can never
    /// match starting on a continuation byte.
    pub(super) fn find_ascii_ci(haystack: &str, needle_lower: &str, from: usize) -> Option<usize> {
        let h = haystack.as_bytes();
        let n = needle_lower.as_bytes();
        if n.is_empty() {
            return (from <= h.len()).then_some(from);
        }
        if from + n.len() > h.len() {
            return None;
        }
        'outer: for i in from..=h.len() - n.len() {
            for (j, &nb) in n.iter().enumerate() {
                if h[i + j].to_ascii_lowercase() != nb {
                    continue 'outer;
                }
            }
            return Some(i);
        }
        None
    }

    /// True when the active search query matches this segment (case-insensitive).
    fn segment_matches_query(segment: &TextSegment, query_lower: Option<&str>) -> bool {
        query_lower.is_some_and(|query| Self::find_ascii_ci(&segment.text, query, 0).is_some())
    }

    /// The active in-window search query (lowercased), if searching.
    /// ASCII lowercasing keeps byte offsets identical to the source text so
    /// match runs can slice it safely.
    pub(super) fn active_search_query(app_core: &AppCore) -> Option<String> {
        let query = app_core.ui_state.search_input.trim();
        if app_core.ui_state.input_mode == InputMode::Search && !query.is_empty() {
            Some(query.to_ascii_lowercase())
        } else {
            None
        }
    }

    /// Split text into (piece, is_match) runs for an ascii-lowercased query.
    pub(super) fn split_search_runs<'t>(text: &'t str, query_lower: &str) -> Vec<(&'t str, bool)> {
        let mut runs = Vec::new();
        if query_lower.is_empty() {
            runs.push((text, false));
            return runs;
        }
        let mut pos = 0;
        while let Some(start) = Self::find_ascii_ci(text, query_lower, pos) {
            let end = start + query_lower.len();
            if start > pos {
                runs.push((&text[pos..start], false));
            }
            runs.push((&text[start..end], true));
            pos = end;
        }
        if pos < text.len() {
            runs.push((&text[pos..], false));
        }
        runs
    }

    /// Text format for a slice of a segment, mirroring segment_to_rich_text.
    fn segment_text_format(
        segment: &TextSegment,
        visuals: &egui::Visuals,
        search_match: bool,
        font_id: &egui::FontId,
    ) -> egui::TextFormat {
        let color = segment
            .fg
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| visuals.text_color());
        let background = if search_match {
            visuals.selection.bg_fill
        } else {
            segment
                .bg
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(Color32::TRANSPARENT)
        };
        egui::TextFormat {
            font_id: egui::FontId {
                size: font_id.size + if segment.bold { 0.5 } else { 0.0 },
                family: if segment.mono {
                    egui::FontFamily::Monospace
                } else {
                    font_id.family.clone()
                },
            },
            color,
            background,
            ..Default::default()
        }
    }

    /// Emit the accumulated non-link text as a single label. One galley per
    /// run (instead of one widget per segment) keeps wrapping natural and
    /// lets egui's galley cache reuse the layout across frames.
    fn flush_text_job(ui: &mut egui::Ui, job: &mut egui::text::LayoutJob) {
        if job.is_empty() {
            return;
        }
        let job = std::mem::take(job);
        ui.add(egui::Label::new(job));
    }

    pub(super) fn render_styled_line(
        ui: &mut egui::Ui,
        line: &StyledLine,
        visuals: &egui::Visuals,
        search_query: Option<&str>,
        font_id: &egui::FontId,
        wrap: bool,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;

        ui.scope(|ui| {
            // Keep inter-widget spacing at zero so links don't introduce
            // artificial spaces around punctuation.
            ui.spacing_mut().item_spacing.x = 0.0;
            if !wrap {
                // One line stays one row; the enclosing scroll area provides
                // horizontal scrolling.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            }

            let row = |ui: &mut egui::Ui| {
                // Consecutive non-link segments accumulate into one LayoutJob;
                // links flush it and render as their own clickable widgets.
                let mut job = egui::text::LayoutJob::default();

                for segment in &line.segments {
                    if segment.text.is_empty() {
                        continue;
                    }

                    let is_link = Self::segment_has_clickable_link(segment);
                    let search_match = Self::segment_matches_query(segment, search_query);

                    if is_link {
                        Self::flush_text_job(ui, &mut job);
                        // Links stay one clickable widget; highlight the whole
                        // segment when it matches. While the drag modifier is
                        // held with the mouse button down, the label is not
                        // selectable text, so starting an item drag never
                        // starts a text selection.
                        let rich = Self::segment_to_rich_text(
                            segment,
                            visuals,
                            is_link,
                            search_match,
                            font_id,
                        );
                        let response = ui
                            .add(
                                egui::Label::new(rich)
                                    .sense(egui::Sense::click_and_drag())
                                    .selectable(!Self::link_drag_blocks_selection(ui)),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if let Some(link_data) = &segment.link_data {
                            if let Some(drop) = Self::handle_link_dnd(ui, &response, link_data) {
                                clicked_link.get_or_insert(drop);
                            }
                        }
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
                    } else if search_match {
                        // Highlight only the matched substrings.
                        let query = search_query.unwrap_or_default();
                        for (piece, is_match) in Self::split_search_runs(&segment.text, query) {
                            job.append(
                                piece,
                                0.0,
                                Self::segment_text_format(segment, visuals, is_match, font_id),
                            );
                        }
                    } else {
                        job.append(
                            &segment.text,
                            0.0,
                            Self::segment_text_format(segment, visuals, false, font_id),
                        );
                    }
                }

                Self::flush_text_job(ui, &mut job);
            };
            if wrap {
                ui.horizontal_wrapped(row);
            } else {
                ui.horizontal(row);
            }
        });

        clicked_link
    }

    /// WCAG relative luminance (0 = black, 1 = white).
    fn relative_luminance(color: Color32) -> f32 {
        fn channel(value: u8) -> f32 {
            let value = value as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    /// WCAG contrast ratio between two colors (1.0 to 21.0).
    fn contrast_ratio(a: Color32, b: Color32) -> f32 {
        let la = Self::relative_luminance(a);
        let lb = Self::relative_luminance(b);
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// Pick a readable text color for text painted over `background`.
    /// Keeps `preferred` when it has enough contrast; otherwise falls back
    /// to near-black or near-white, whichever contrasts with the background.
    pub(super) fn readable_text_color(
        preferred: Color32,
        background: Color32,
        auto_contrast: bool,
    ) -> Color32 {
        // 3.0 is the WCAG minimum for large text; bar labels are short and
        // bold enough that this is a reasonable floor.
        if !auto_contrast || Self::contrast_ratio(preferred, background) >= 3.0 {
            return preferred;
        }
        if Self::relative_luminance(background) > 0.35 {
            Color32::from_rgb(0x14, 0x14, 0x14)
        } else {
            Color32::from_rgb(0xf2, 0xf2, 0xf2)
        }
    }

    /// Paint a galley twice, clipped at `boundary_x` (the bar's fill edge):
    /// glyphs left of the boundary use `over_fill`, glyphs right of it use
    /// `over_trough`, so text straddling the edge stays readable on both
    /// sides. Single paint when the colors agree. The galley must be laid
    /// out with `Color32::PLACEHOLDER` so the per-side color applies.
    fn paint_split_galley(
        painter: &egui::Painter,
        clip: Rect,
        pos: Pos2,
        galley: std::sync::Arc<egui::Galley>,
        boundary_x: f32,
        over_fill: Color32,
        over_trough: Color32,
    ) {
        if over_fill == over_trough {
            painter.with_clip_rect(clip).galley(pos, galley, over_fill);
            return;
        }
        let left = Rect::from_min_max(
            clip.min,
            Pos2::new(boundary_x.clamp(clip.min.x, clip.max.x), clip.max.y),
        );
        let right = Rect::from_min_max(
            Pos2::new(boundary_x.clamp(clip.min.x, clip.max.x), clip.min.y),
            clip.max,
        );
        if left.width() > 0.0 {
            painter
                .with_clip_rect(left)
                .galley(pos, galley.clone(), over_fill);
        }
        if right.width() > 0.0 {
            painter.with_clip_rect(right).galley(pos, galley, over_trough);
        }
    }

    /// A progress bar with the user's corner radius and readable centered
    /// text. Centered text sits over the fill once the bar is half full and
    /// over the trough below that, so contrast is checked against whichever
    /// is behind it.
    fn styled_progress_bar(
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

    /// Bar text for a vital with a true value/max pair (the core four).
    fn vital_bar_text(
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
    fn vital_status_text(
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
            .unwrap_or_else(|| ui.visuals().selection.bg_fill);
        let text = if data.label.is_empty() {
            format!("{}%", (fraction * 100.0).round() as u32)
        } else {
            data.label.clone()
        };
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let fraction = Self::animated_fraction(ui, &data.progress_id, fraction);
        let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
        ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
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
            ui.label("No vitals selected (Settings → GUI → Vitals).");
            return;
        }

        let bar_height = config.bar_height.clamp(8.0, 60.0);
        match config.orientation {
            VitalsOrientation::Horizontal => {
                ui.columns(bars.len(), |columns| {
                    for (column, (id, fraction, text, fill)) in
                        columns.iter_mut().zip(bars.into_iter())
                    {
                        let fraction = Self::animated_fraction(column, id, fraction);
                        let bar = Self::styled_progress_bar(column, settings, fraction, fill, text);
                        column.add_sized([column.available_width().max(40.0), bar_height], bar);
                    }
                });
            }
            VitalsOrientation::Vertical => {
                for (id, fraction, text, fill) in bars {
                    let fraction = Self::animated_fraction(ui, id, fraction);
                    let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
                    ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
                }
            }
        }
    }

    /// Remaining whole seconds on a countdown, adjusted for server clock drift.
    pub(super) fn countdown_remaining_seconds(
        end_time: i64,
        server_time_offset: i64,
        local_unix_time: i64,
    ) -> u32 {
        (end_time - (local_unix_time + server_time_offset)).max(0) as u32
    }

    /// Fractional remaining seconds on a countdown, so the drain bar moves a
    /// little on every repaint instead of stepping once per whole second.
    fn countdown_remaining_seconds_f(
        end_time: i64,
        server_time_offset: i64,
        local_unix_time_f: f64,
    ) -> f32 {
        ((end_time - server_time_offset) as f64 - local_unix_time_f).max(0.0) as f32
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
            now_f as i64,
        );

        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let bar_width = ui.available_width().max(40.0);
        if remaining == 0 {
            // Idle timers render blank, matching the TUI.
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
        let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
        ui.add_sized([bar_width, bar_height], bar);
    }

    /// Body-part glyph grid mirroring the TUI injury doll (col, glyph, part).
    const INJURY_DOLL_ROWS: &'static [&'static [(usize, &'static str, &'static str)]] = &[
        &[(0, "\u{2022}", "leftEye"), (4, "\u{2022}", "rightEye")],
        &[(2, "0", "head"), (6, "nk", "neck")],
        &[(1, "/", "leftArm"), (2, "|", "chest"), (3, "\\", "rightArm")],
        &[
            (0, "o", "leftHand"),
            (2, "|", "abdomen"),
            (4, "o", "rightHand"),
            (6, "bk", "back"),
        ],
        &[(1, "/", "leftLeg"), (3, "\\", "rightLeg")],
        &[(0, "o", "leftLeg"), (4, "o", "rightLeg"), (6, "ns", "nsys")],
    ];

    /// ProfanityFE injury palette: none, injury 1-3, scar 1-3.
    pub(super) fn injury_level_color(level: u8) -> Color32 {
        match level.min(6) {
            0 => Color32::from_rgb(0x33, 0x33, 0x33),
            1 => Color32::from_rgb(0xaa, 0x55, 0x00),
            2 => Color32::from_rgb(0xff, 0x88, 0x00),
            3 => Color32::from_rgb(0xff, 0x00, 0x00),
            4 => Color32::from_rgb(0x99, 0x99, 0x99),
            5 => Color32::from_rgb(0x77, 0x77, 0x77),
            _ => Color32::from_rgb(0x55, 0x55, 0x55),
        }
    }

    pub(super) fn render_injury_doll_grid(
        ui: &mut egui::Ui,
        injuries: &HashMap<String, u8>,
    ) {
        for row in Self::INJURY_DOLL_ROWS {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let mut col = 0usize;
                for (start, glyph, part) in row.iter() {
                    if *start > col {
                        ui.label(RichText::new(" ".repeat(start - col)).monospace());
                    }
                    let level = injuries.get(*part).copied().unwrap_or(0);
                    ui.label(
                        RichText::new(*glyph)
                            .monospace()
                            .color(Self::injury_level_color(level)),
                    );
                    col = start + glyph.chars().count();
                }
            });
        }
    }

    /// Popup for viewing another player's injuries (server `injuries-*` dialog).
    pub(super) fn render_injuries_popup(&mut self, ctx: &egui::Context) {
        let Some(popup) = self.app_core.ui_state.injuries_popup.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new(format!("{}'s Injuries", popup.player_name))
            .id(egui::Id::new("gui_injuries_popup"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                Self::render_injury_doll_grid(ui, &popup.injuries);
            });
        if !open {
            self.app_core.ui_state.injuries_popup = None;
        }
    }

    pub(super) fn render_indicator_content(
        ui: &mut egui::Ui,
        label: &str,
        indicator: &crate::data::IndicatorData,
    ) {
        let text = if label.is_empty() {
            &indicator.indicator_id
        } else {
            label
        };
        // TUI defaults: #00ff00 when active, #555555 when off.
        let color = if indicator.active {
            indicator
                .color
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(Color32::from_rgb(0x00, 0xff, 0x00))
        } else {
            Color32::from_rgb(0x55, 0x55, 0x55)
        };
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(text).color(color).strong());
        });
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
        link: &Option<LinkData>,
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

        // Held items carry server link data; render them clickable like other links.
        let item_link = if item_text == empty_text {
            None
        } else {
            link.as_ref()
        };

        let mut clicked_link = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_sized(
                [icon_width, row_height],
                egui::Label::new(RichText::new(icon_text).monospace().strong()),
            );
            ui.add_space(icon_gap);
            let text_width = (ui.available_width() - handle_gutter_width).max(1.0);
            if let Some(link_data) = item_link {
                let response = ui
                    .add_sized(
                        [text_width, row_height],
                        egui::Label::new(display_text)
                            .truncate()
                            .sense(egui::Sense::click_and_drag())
                            .selectable(!Self::link_drag_blocks_selection(ui)),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                // Drag source only: releases over hand windows resolve at the
                // window level to `left`/`right`, never onto the held item.
                if Self::link_is_draggable(link_data) && Self::link_drag_modifier_down(ui) {
                    response.dnd_set_drag_payload(link_data.clone());
                }
                if response.clicked() {
                    clicked_link = Some(Self::gui_link_click_from_response(
                        &response,
                        ui,
                        link_data.clone(),
                    ));
                }
            } else {
                ui.add_sized(
                    [text_width, row_height],
                    egui::Label::new(display_text).truncate(),
                );
            }
            ui.add_space(handle_gutter_width);
        });

        clicked_link
    }

    pub(super) fn render_gs4_experience_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
    ) {
        let exp = &app_core.game_state.gs4_experience;
        if exp.level_text.is_empty() && exp.mind_state_text.is_empty() && exp.next_level_text.is_empty() {
            ui.weak("No experience data yet.");
            return;
        }

        if !exp.level_text.is_empty() {
            ui.label(RichText::new(&exp.level_text).strong());
        }
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        if !exp.mind_state_text.is_empty() {
            let fraction =
                Self::animated_fraction(ui, "gs4_mind", exp.mind_state_value.min(100) as f32 / 100.0);
            let bar = Self::styled_progress_bar(
                ui,
                settings,
                fraction,
                Color32::from_rgb(0x47, 0x84, 0xd9),
                format!("Mind: {}", exp.mind_state_text),
            );
            ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
        }
        if !exp.next_level_text.is_empty() {
            let fraction =
                Self::animated_fraction(ui, "gs4_next", exp.next_level_value.min(100) as f32 / 100.0);
            let bar = Self::styled_progress_bar(
                ui,
                settings,
                fraction,
                Color32::from_rgb(0x55, 0xb8, 0x6c),
                format!("Next: {}", exp.next_level_text),
            );
            ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
        }
    }

    pub(super) fn render_dr_experience_content(app_core: &AppCore, ui: &mut egui::Ui) {
        let fields = app_core.game_state.dr_experience.fields_with_values();
        if fields.is_empty() {
            ui.weak("No experience data yet.");
            return;
        }

        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("dr_experience_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for (name, value) in fields {
                    ui.label(RichText::new(format!("{}: {}", name, value)).monospace());
                }
            });
    }

    pub(super) fn render_encumbrance_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
    ) {
        let enc = &app_core.game_state.encumbrance;
        let value = enc.value.min(100);
        let fill = match value {
            0..=33 => Color32::from_rgb(0x55, 0xb8, 0x6c),
            34..=66 => Color32::from_rgb(0xff, 0x88, 0x00),
            _ => Color32::from_rgb(0xcd, 0x4d, 0x4d),
        };
        let text = if enc.text.is_empty() {
            format!("Encumbrance: {}%", value)
        } else {
            format!("Encumbrance: {}", enc.text)
        };
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let fraction = Self::animated_fraction(ui, "encumbrance", value as f32 / 100.0);
        let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
        ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
        if !enc.blurb.is_empty() {
            ui.weak(&enc.blurb);
        }
    }

    pub(super) fn render_betrayer_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
    ) {
        let betrayer = &app_core.game_state.betrayer;
        let text = if betrayer.text.is_empty() {
            format!("Blood Points: {}", betrayer.value)
        } else {
            betrayer.text.clone()
        };
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let fraction =
            Self::animated_fraction(ui, "betrayer", betrayer.value.min(100) as f32 / 100.0);
        let bar = Self::styled_progress_bar(
            ui,
            settings,
            fraction,
            Color32::from_rgb(0xcd, 0x4d, 0x4d),
            text,
        );
        ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
        if !betrayer.items.is_empty() {
            let max_height = ui.available_height().max(1.0);
            egui::ScrollArea::vertical()
                .id_salt("betrayer_scroll")
                .auto_shrink([false, false])
                .min_scrolled_height(max_height)
                .max_height(max_height)
                .show(ui, |ui| {
                    for item in &betrayer.items {
                        ui.label(item);
                    }
                });
        }
    }

    pub(super) fn render_perception_content(
        ui: &mut egui::Ui,
        perception: &crate::data::PerceptionData,
    ) -> Option<GuiLinkClick> {
        if perception.entries.is_empty() {
            ui.weak("Nothing perceived.");
            return None;
        }

        let mut clicked_link = None;
        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("perception_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for entry in &perception.entries {
                    if let Some(link_data) = &entry.link_data {
                        let response = ui
                            .add(
                                egui::Label::new(entry.raw_text.as_str())
                                    .sense(egui::Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() && clicked_link.is_none() {
                            clicked_link = Some(Self::gui_link_click_from_response(
                                &response,
                                ui,
                                link_data.clone(),
                            ));
                        }
                    } else {
                        ui.label(entry.raw_text.as_str());
                    }
                }
            });
        clicked_link
    }

    pub(super) fn render_items_content(app_core: &AppCore, ui: &mut egui::Ui) -> Option<GuiLinkClick> {
        let objects = &app_core.game_state.room_objects;
        if objects.is_empty() {
            ui.weak("No objects here.");
            return None;
        }

        let mut clicked_link = None;
        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("items_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for object in objects {
                    let object_link = LinkData {
                        exist_id: object.id.clone(),
                        noun: object.noun.clone().unwrap_or_default(),
                        text: object.name.clone(),
                        coord: None,
                    };
                    let response = ui
                        .add(
                            egui::Label::new(object.name.as_str())
                                .sense(egui::Sense::click_and_drag())
                                .selectable(!Self::link_drag_blocks_selection(ui)),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if let Some(drop) = Self::handle_link_dnd(ui, &response, &object_link) {
                        clicked_link.get_or_insert(drop);
                    }
                    if response.clicked() && clicked_link.is_none() {
                        clicked_link = Some(Self::gui_link_click_from_response(
                            &response,
                            ui,
                            object_link,
                        ));
                    }
                }
            });
        clicked_link
    }

    pub(super) fn render_container_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        container_title: &str,
        wrap: bool,
    ) {
        let Some(container) = app_core.game_state.container_cache.find_by_title(container_title)
        else {
            ui.weak(format!("No contents cached for \"{}\".", container_title));
            return;
        };

        if container.items.is_empty() {
            ui.weak("Empty.");
            return;
        }

        let max_height = ui.available_height().max(1.0);
        let scroll_area = if wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };
        scroll_area
            .id_salt(format!("container_scroll_{}", container.id))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                if !wrap {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                }
                for item in &container.items {
                    ui.label(item);
                }
            });
    }

    /// Sentinel exist_id used to route quickbar switching through the
    /// link-click channel (content renderers only get `&AppCore`).
    pub(super) const QUICKBAR_SWITCH_SENTINEL: &'static str = "_quickbar_switch_";

    /// Sentinel exist_id for an item dropped onto another link;
    /// noun is "<dragged_exist_id>|<target_exist_id>".
    pub(super) const LINK_DROP_SENTINEL: &'static str = "_link_drop_";

    /// egui temp-data key holding the configured item-drag modifier.
    pub(super) fn drag_modifier_data_id() -> egui::Id {
        egui::Id::new("vellum_drag_modifier")
    }

    /// True while exactly the configured item-drag modifier (default Ctrl) is
    /// held. Exact matching keeps combined modifiers (e.g. Ctrl+Shift) free
    /// for keybinds and prevents AltGr (reported as Ctrl+Alt on Windows
    /// international layouts) from triggering Ctrl drags.
    fn link_drag_modifier_down(ui: &egui::Ui) -> bool {
        let required: egui::Modifiers = ui
            .ctx()
            .data(|data| data.get_temp(Self::drag_modifier_data_id()))
            .unwrap_or(egui::Modifiers::CTRL);
        ui.input(|input| input.modifiers.matches_exact(required))
    }

    /// True while a modifier+drag on a link must not start a text selection:
    /// the drag modifier is held AND the primary button is down. The button
    /// check matters — suppressing on the modifier alone made link labels
    /// non-selectable on the Ctrl+C frame (the default modifier is Ctrl), so
    /// egui silently dropped link text from copied selections.
    fn link_drag_blocks_selection(ui: &egui::Ui) -> bool {
        Self::link_drag_modifier_down(ui)
            && ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
    }

    /// Only real game entities can be dragged (not command/sentinel links).
    fn link_is_draggable(link: &LinkData) -> bool {
        !link.exist_id.trim().is_empty() && !link.exist_id.starts_with('_')
    }

    /// Shared drag-source + drop-target handling for a link widget.
    /// Returns a drop event when another item was released onto this link.
    fn handle_link_dnd(
        ui: &egui::Ui,
        response: &egui::Response,
        link_data: &LinkData,
    ) -> Option<GuiLinkClick> {
        if Self::link_is_draggable(link_data) && Self::link_drag_modifier_down(ui) {
            response.dnd_set_drag_payload(link_data.clone());
        }
        if Self::link_is_draggable(link_data) {
            if let Some(dragged) = response.dnd_release_payload::<LinkData>() {
                if dragged.exist_id != link_data.exist_id {
                    return Some(GuiLinkClick {
                        link_data: LinkData {
                            exist_id: Self::LINK_DROP_SENTINEL.to_string(),
                            noun: format!("{}|{}", dragged.exist_id, link_data.exist_id),
                            text: String::new(),
                            coord: None,
                        },
                        click_pos: (0, 0),
                    });
                }
            }
        }
        None
    }

    /// Sentinel exist_id for switching the active tab of a tabbedtext window;
    /// noun is "<window_name>|<tab_index>".
    pub(super) const TABBED_SWITCH_SENTINEL: &'static str = "_tabbed_switch_";

    /// Inner tab strip for tabbedtext windows. Unread tabs render bold; clicks
    /// flow through the link channel since renderers only get `&AppCore`.
    fn render_tabbed_text_tab_strip(
        ui: &mut egui::Ui,
        window_name: &str,
        tabbed: &TabbedTextContent,
    ) -> Option<GuiLinkClick> {
        if tabbed.tabs.len() < 2 {
            return None;
        }
        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            for (index, tab_state) in tabbed.tabs.iter().enumerate() {
                let is_active = index == tabbed.active_tab_index;
                let mut label = RichText::new(&tab_state.definition.name);
                if tab_state.has_unread && !is_active {
                    label = label.strong();
                }
                if ui.selectable_label(is_active, label).clicked() && !is_active {
                    clicked = Some(GuiLinkClick {
                        link_data: LinkData {
                            exist_id: Self::TABBED_SWITCH_SENTINEL.to_string(),
                            noun: format!("{}|{}", window_name, index),
                            text: String::new(),
                            coord: None,
                        },
                        click_pos: (0, 0),
                    });
                }
            }
        });
        ui.separator();
        clicked
    }

    pub(super) fn render_quickbar_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
    ) -> Option<GuiLinkClick> {
        let ui_state = &app_core.ui_state;
        if ui_state.quickbars.is_empty() {
            ui.weak("No quickbars configured.");
            return None;
        }

        let mut ids: Vec<&String> = ui_state.quickbars.keys().collect();
        ids.sort();
        let active_id = ui_state
            .active_quickbar_id
            .as_ref()
            .filter(|id| ui_state.quickbars.contains_key(*id))
            .cloned()
            .unwrap_or_else(|| ids[0].clone());
        let quickbar = &ui_state.quickbars[&active_id];
        let quickbar_title = |id: &String| {
            ui_state.quickbars[id]
                .title
                .clone()
                .unwrap_or_else(|| id.clone())
        };

        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            if ids.len() > 1 {
                let mut selected = active_id.clone();
                egui::ComboBox::from_id_salt("quickbar_switcher")
                    .selected_text(quickbar_title(&active_id))
                    .show_ui(ui, |ui| {
                        for id in &ids {
                            ui.selectable_value(&mut selected, (*id).clone(), quickbar_title(id));
                        }
                    });
                if selected != active_id && clicked.is_none() {
                    clicked = Some(GuiLinkClick {
                        link_data: LinkData {
                            exist_id: Self::QUICKBAR_SWITCH_SENTINEL.to_string(),
                            noun: selected,
                            text: String::new(),
                            coord: None,
                        },
                        click_pos: (0, 0),
                    });
                }
                ui.separator();
            }

            for entry in &quickbar.entries {
                match entry {
                    crate::data::QuickbarEntry::Label { value, .. } => {
                        ui.label(value);
                    }
                    crate::data::QuickbarEntry::Link { value, cmd, .. } => {
                        let response = ui.button(value);
                        if response.clicked() && clicked.is_none() {
                            clicked = Some(Self::gui_link_click_from_response(
                                &response,
                                ui,
                                Self::direct_command_link(cmd.clone()),
                            ));
                        }
                    }
                    crate::data::QuickbarEntry::MenuLink {
                        value, exist, noun, ..
                    } => {
                        let response = ui.button(value);
                        if response.clicked() && clicked.is_none() {
                            clicked = Some(Self::gui_link_click_from_response(
                                &response,
                                ui,
                                LinkData {
                                    exist_id: exist.clone(),
                                    noun: noun.clone(),
                                    text: value.clone(),
                                    coord: None,
                                },
                            ));
                        }
                    }
                    crate::data::QuickbarEntry::Separator => {
                        ui.separator();
                    }
                }
            }
        });
        clicked
    }

    pub(super) fn render_performance_content(app_core: &AppCore, ui: &mut egui::Ui) {
        let cfg = app_core.perf_overlay_data(true);
        let stats = &app_core.perf_stats;

        let mut rows: Vec<(&str, String)> = Vec::new();
        if cfg.show_fps {
            rows.push(("FPS", format!("{:.1}", stats.fps())));
        }
        if cfg.show_frame_times {
            rows.push((
                "Frame",
                format!(
                    "{:.2} ms ({:.2}-{:.2})",
                    stats.avg_frame_time_ms(),
                    stats.min_frame_time_ms(),
                    stats.max_frame_time_ms()
                ),
            ));
        }
        if cfg.show_render_times {
            rows.push(("Render", format!("{:.2} ms", stats.avg_render_time_ms())));
        }
        if cfg.show_ui_times {
            rows.push(("UI", format!("{:.2} ms", stats.avg_ui_render_time_ms())));
        }
        if cfg.show_wrap_times {
            rows.push(("Wrap", format!("{:.1} us", stats.avg_text_wrap_time_us())));
        }
        if cfg.show_net {
            rows.push((
                "Net",
                format!(
                    "{} B/s in, {} B/s out",
                    stats.bytes_received_per_sec(),
                    stats.bytes_sent_per_sec()
                ),
            ));
        }
        if cfg.show_parse {
            rows.push((
                "Parse",
                format!(
                    "{:.1} us, {} elem/s",
                    stats.avg_parse_time_us(),
                    stats.elements_per_sec()
                ),
            ));
        }
        if cfg.show_events {
            rows.push((
                "Events",
                format!(
                    "{:.1} us, queue {}",
                    stats.avg_event_process_time_us(),
                    stats.last_event_queue_depth()
                ),
            ));
        }
        if cfg.show_memory {
            rows.push((
                "Memory",
                format!(
                    "{:.1} MB rss, {:.1} MB est",
                    stats.process_rss_mb(),
                    stats.estimated_memory_mb()
                ),
            ));
        }
        if cfg.show_lines {
            rows.push((
                "Lines",
                format!(
                    "{} in {} windows",
                    stats.total_lines_buffered(),
                    stats.active_window_count()
                ),
            ));
        }
        if cfg.show_uptime {
            rows.push(("Uptime", stats.uptime_formatted()));
        }
        if cfg.show_jitter {
            rows.push(("Jitter", format!("{:.2} ms", stats.frame_jitter_ms())));
        }
        if cfg.show_frame_spikes {
            rows.push(("Spikes", stats.frame_spike_count().to_string()));
        }
        if cfg.show_event_lag {
            rows.push(("Event lag", format!("{:.1} ms", stats.event_lag_ms())));
        }
        if cfg.show_memory_delta {
            rows.push(("Mem delta", format!("{:+.1} MB", stats.memory_delta_mb())));
        }

        if rows.is_empty() {
            ui.weak("All performance metrics are disabled in settings.");
            return;
        }

        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("performance_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for (name, value) in rows {
                    ui.label(RichText::new(format!("{:<10} {}", name, value)).monospace());
                }
            });
    }

    pub(super) fn render_dashboard_content(ui: &mut egui::Ui, indicators: &[(String, u8)]) {
        // Matches the TUI dashboard default of hiding inactive indicators.
        let active: Vec<&(String, u8)> = indicators
            .iter()
            .filter(|(_, value)| *value > 0)
            .collect();
        if active.is_empty() {
            ui.weak("No active status.");
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for (id, value) in active {
                let color = match value {
                    1 => Color32::from_rgb(0x55, 0xb8, 0x6c),
                    2 => Color32::from_rgb(0xff, 0x88, 0x00),
                    _ => Color32::from_rgb(0xcd, 0x4d, 0x4d),
                };
                ui.label(RichText::new(id).color(color).strong());
            }
        });
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

    /// Wrayth/TUI-style effect rows: each effect is a single fixed-height
    /// bar whose fill tracks remaining duration, with the name overlaid on
    /// the left and the time on the right. Row height and text size are
    /// user-adjustable (Settings → GUI, per-window text size).
    pub(super) fn render_active_effects_content(
        ui: &mut egui::Ui,
        effects_content: &crate::data::ActiveEffectsContent,
        settings: WidgetRenderSettings,
    ) {
        if effects_content.effects.is_empty() {
            ui.label(format!("No active {}.", effects_content.category));
            return;
        }

        let row_height = settings.effects_bar_height;
        let text_size = settings.text_size.min(row_height - 2.0).max(6.0);
        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt(format!("active_effects_{}", effects_content.category))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for effect in &effects_content.effects {
                    let desired = Vec2::new(ui.available_width().max(1.0), row_height);
                    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::hover());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }

                    let visuals = ui.visuals();
                    let bg = visuals.extreme_bg_color;
                    let fill = effect
                        .bar_color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(visuals.selection.bg_fill);
                    let preferred_text_color = effect
                        .text_color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or_else(|| visuals.text_color());

                    let corner_radius = settings.bar_corner_radius;
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, corner_radius, bg);
                    let fraction = (effect.value.min(100) as f32) / 100.0;
                    if fraction > 0.0 {
                        let fill_rect = Rect::from_min_size(
                            rect.min,
                            Vec2::new(rect.width() * fraction, rect.height()),
                        );
                        painter.rect_filled(fill_rect, corner_radius, fill);
                    }

                    // Text is painted in two clipped passes split at the fill
                    // edge, so a duration straddling the boundary is
                    // contrast-checked against the fill on its left half and
                    // the trough on its right half.
                    let boundary_x = rect.left() + rect.width() * fraction;
                    let over_fill = Self::readable_text_color(
                        preferred_text_color,
                        fill,
                        settings.auto_contrast_bar_text,
                    );
                    let over_trough = Self::readable_text_color(
                        preferred_text_color,
                        bg,
                        settings.auto_contrast_bar_text,
                    );

                    // Time on the right; the name is clipped so it never
                    // paints under the time.
                    let font = egui::FontId {
                        size: text_size,
                        family: settings.font_family.clone(),
                    };
                    let time = effect.time.trim();
                    let mut name_clip = rect.shrink2(Vec2::new(4.0, 0.0));
                    if !time.is_empty() {
                        let time_galley = painter.layout_no_wrap(
                            time.to_string(),
                            font.clone(),
                            Color32::PLACEHOLDER,
                        );
                        let time_pos = Pos2::new(
                            rect.right() - 4.0 - time_galley.size().x,
                            rect.center().y - time_galley.size().y / 2.0,
                        );
                        Self::paint_split_galley(
                            &painter,
                            rect,
                            time_pos,
                            time_galley.clone(),
                            boundary_x,
                            over_fill,
                            over_trough,
                        );
                        name_clip.max.x =
                            (rect.right() - 8.0 - time_galley.size().x).max(name_clip.min.x);
                    }
                    let name_galley = painter.layout_no_wrap(
                        effect.text.clone(),
                        font,
                        Color32::PLACEHOLDER,
                    );
                    let name_pos = Pos2::new(
                        name_clip.min.x,
                        rect.center().y - name_galley.size().y / 2.0,
                    );
                    Self::paint_split_galley(
                        &painter,
                        name_clip,
                        name_pos,
                        name_galley,
                        boundary_x,
                        over_fill,
                        over_trough,
                    );

                    // Narrow windows clip the name; hover shows the full text.
                    if !effect.text.is_empty() {
                        let hover = if time.is_empty() {
                            effect.text.clone()
                        } else {
                            format!("{} — {}", effect.text, time)
                        };
                        response.on_hover_text(hover);
                    }
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

    /// Estimated height of one line at the given wrap width, from a single
    /// LayoutJob over all segments. Exact for link-free lines (they render as
    /// one galley); link-bearing lines wrap as separate widgets and may
    /// differ slightly — the renderer self-corrects those once visible.
    fn measure_line_height(
        ctx: &egui::Context,
        line: &StyledLine,
        visuals: &egui::Visuals,
        wrap_width: f32,
        font_id: &egui::FontId,
    ) -> f32 {
        let mut job = egui::text::LayoutJob {
            wrap: egui::text::TextWrapping {
                max_width: wrap_width,
                ..Default::default()
            },
            ..Default::default()
        };
        for segment in &line.segments {
            if segment.text.is_empty() {
                continue;
            }
            job.append(
                &segment.text,
                0.0,
                Self::segment_text_format(segment, visuals, false, font_id),
            );
        }
        if job.is_empty() {
            // Blank line: renders as an empty row; estimate one text row and
            // let the on-render correction settle the exact value.
            return ctx.fonts_mut(|fonts| fonts.row_height(font_id));
        }
        ctx.fonts_mut(|fonts| fonts.layout_job(job)).size().y
    }

    /// Bring the height cache in sync with the rendered slice
    /// `content.lines[start..start + rendered_count]`. Appends measure only
    /// the new lines; width changes or non-monotonic generations rebuild.
    fn update_row_height_cache(
        cache: &mut RowHeightCache,
        ctx: &egui::Context,
        content: &TextContent,
        start: usize,
        rendered_count: usize,
        wrap_width: f32,
        visuals: &egui::Visuals,
        font_id: &egui::FontId,
    ) {
        let width_changed =
            (cache.wrap_width - wrap_width).abs() > 0.5 || cache.font_id != *font_id;
        let delta = content.generation.wrapping_sub(cache.generation) as usize;
        let incremental = !width_changed
            && content.generation >= cache.generation
            && delta <= rendered_count
            && cache.heights.len() + delta >= rendered_count;

        if incremental {
            if delta > 0 {
                let drop_front = (cache.heights.len() + delta).saturating_sub(rendered_count);
                cache.heights.drain(..drop_front.min(cache.heights.len()));
                let len = content.lines.len();
                for line in content.lines.iter().skip(len - delta) {
                    cache.heights.push(Self::measure_line_height(
                        ctx, line, visuals, wrap_width, font_id,
                    ));
                }
            }
        } else {
            cache.heights.clear();
            cache.heights.reserve(rendered_count);
            for line in content.lines.iter().skip(start) {
                cache.heights.push(Self::measure_line_height(
                    ctx, line, visuals, wrap_width, font_id,
                ));
            }
        }
        cache.wrap_width = wrap_width;
        cache.font_id = font_id.clone();
        cache.generation = content.generation;
        debug_assert_eq!(cache.heights.len(), rendered_count);
    }

    pub(super) fn render_text_content(
        ui: &mut egui::Ui,
        content: &TextContent,
        scroll_id: &str,
        search_query: Option<&str>,
        font_id: &egui::FontId,
        wrap: bool,
    ) -> Option<GuiLinkClick> {
        // Cheap Arc clone; deep-cloning Visuals per window per frame is not.
        let style = ui.style().clone();
        let visuals = &style.visuals;
        let mut clicked_link = None;
        let rendered_count = content.lines.len().min(MAX_RENDERED_LINES);
        let start = content.lines.len() - rendered_count;
        let max_height = ui.available_height().max(1.0);
        let cache_id = egui::Id::new(("text_row_heights", scroll_id));

        let scroll_area = if wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };
        scroll_area
            .id_salt(format!("text_scroll_{}", scroll_id))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show_viewport(ui, |ui, viewport| {
                if rendered_count == 0 {
                    return;
                }
                let ctx = ui.ctx().clone();
                let wrap_width = if wrap {
                    ui.available_width().max(1.0)
                } else {
                    f32::INFINITY
                };
                let spacing_y = ui.spacing().item_spacing.y;

                // The cache lives in egui temp data so renderers stay
                // stateless; the Arc dance keeps ctx.fonts_mut() callable
                // while the cache is borrowed (calling it inside ctx.data_mut
                // would deadlock on the context lock).
                let cache_handle = ctx.data_mut(|data| {
                    data.get_temp_mut_or_insert_with::<std::sync::Arc<
                        std::sync::Mutex<RowHeightCache>,
                    >>(cache_id, Default::default)
                        .clone()
                });
                let mut cache = cache_handle.lock().expect("row height cache poisoned");
                Self::update_row_height_cache(
                    &mut cache,
                    &ctx,
                    content,
                    start,
                    rendered_count,
                    wrap_width,
                    &visuals,
                    font_id,
                );

                // Visible index range from cumulative strides (height +
                // vertical item spacing). Only those lines become widgets;
                // the rest are stand-in spacers.
                let top = viewport.min.y.max(0.0);
                let bottom = viewport.max.y.max(top);
                let mut first_visible = rendered_count;
                let mut top_space = 0.0f32;
                let mut y = 0.0f32;
                for (i, h) in cache.heights.iter().enumerate() {
                    let stride = h + spacing_y;
                    if y + stride > top {
                        first_visible = i;
                        top_space = y;
                        break;
                    }
                    y += stride;
                }
                let mut last_visible = rendered_count;
                let mut yy = top_space;
                for i in first_visible..rendered_count {
                    if yy > bottom {
                        last_visible = i;
                        break;
                    }
                    yy += cache.heights[i] + spacing_y;
                }

                if first_visible > 0 && top_space > spacing_y {
                    // The spacer's trailing item_spacing stands in for the
                    // last skipped line's own spacing.
                    ui.allocate_space(Vec2::new(1.0, top_space - spacing_y));
                }
                for (offset, line) in content
                    .lines
                    .iter()
                    .skip(start + first_visible)
                    .take(last_visible.saturating_sub(first_visible))
                    .enumerate()
                {
                    let before = ui.cursor().min.y;
                    if let Some(link) =
                        Self::render_styled_line(ui, line, &visuals, search_query, font_id, wrap)
                    {
                        clicked_link = Some(link);
                    }
                    // Self-correct the estimate with the rendered height so
                    // spacers stay aligned with reality (see
                    // measure_line_height on why link lines can differ).
                    let actual = ui.cursor().min.y - before - spacing_y;
                    let slot = first_visible + offset;
                    if actual.is_finite()
                        && actual >= 0.0
                        && (cache.heights[slot] - actual).abs() > 0.5
                    {
                        cache.heights[slot] = actual;
                    }
                }
                let bottom_space: f32 = cache.heights[last_visible..]
                    .iter()
                    .map(|h| h + spacing_y)
                    .sum();
                if bottom_space > spacing_y {
                    ui.allocate_space(Vec2::new(1.0, bottom_space - spacing_y));
                }
            });
        clicked_link
    }

    pub(super) fn render_room_description(
        ui: &mut egui::Ui,
        lines: &[StyledLine],
        scroll_id: &str,
        font_id: &egui::FontId,
    ) -> Option<GuiLinkClick> {
        // Cheap Arc clone; deep-cloning Visuals per window per frame is not.
        let style = ui.style().clone();
        let visuals = &style.visuals;
        let mut clicked_link = None;
        let max_height = ui.available_height().max(1.0);

        egui::ScrollArea::vertical()
            .id_salt(format!("room_scroll_{}", scroll_id))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for line in lines {
                    if let Some(link) =
                        Self::render_styled_line(ui, line, &visuals, None, font_id, true)
                    {
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
        settings: WidgetRenderSettings,
    ) -> Option<GuiLinkClick> {
        let Some(window) = app_core.ui_state.windows.get(&tab.window_name) else {
            ui.label("This tab's source window is no longer available.");
            return None;
        };

        // Scale the label-driven text styles so list/grid widgets (targets,
        // players, dashboards, ...) follow the window's text size and font,
        // not just the segment-based text renderers below.
        let text_size = settings.text_size;
        let font_id = settings.font_id();
        {
            let styles = &mut ui.style_mut().text_styles;
            if let Some(font) = styles.get_mut(&egui::TextStyle::Body) {
                font.size = text_size;
                font.family = font_id.family.clone();
            }
            if let Some(font) = styles.get_mut(&egui::TextStyle::Monospace) {
                font.size = text_size;
            }
            if let Some(font) = styles.get_mut(&egui::TextStyle::Small) {
                font.size = (text_size - 4.0).max(8.0);
            }
        }

        match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Spells(content) => {
                let query = Self::active_search_query(app_core);
                Self::render_text_content(
                    ui,
                    content,
                    &tab.window_name,
                    query.as_deref(),
                    &font_id,
                    settings.wrap_text,
                )
            }
            WindowContent::MiniVitals => {
                Self::render_vitals_content(app_core, ui, &settings);
                None
            }
            WindowContent::Progress(data) => {
                Self::render_single_progress_content(ui, data, &settings);
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
                let mut clicked_link =
                    Self::render_tabbed_text_tab_strip(ui, &tab.window_name, tabbed);
                if let Some(active) = tabbed.tabs.get(tabbed.active_tab_index) {
                    let query = Self::active_search_query(app_core);
                    // Per-tab scroll id: each tab keeps its own scroll
                    // position and height cache (tabs have independent
                    // buffers and generations).
                    let scroll_id =
                        format!("{}::tab{}", tab.window_name, tabbed.active_tab_index);
                    if let Some(link) = Self::render_text_content(
                        ui,
                        &active.content,
                        &scroll_id,
                        query.as_deref(),
                        &font_id,
                        settings.wrap_text,
                    ) {
                        clicked_link.get_or_insert(link);
                    }
                } else {
                    ui.label("No active tab content.");
                }
                clicked_link
            }
            WindowContent::Room(room) => {
                // Explicit size: room names track the window's text size, not
                // the Heading style (which the title-bar size setting owns).
                ui.label(
                    RichText::new(&room.name)
                        .font(egui::FontId {
                            size: text_size + 2.0,
                            family: font_id.family.clone(),
                        })
                        .strong(),
                );
                ui.separator();
                let mut clicked_link = Self::render_room_description(
                    ui,
                    &room.description,
                    &tab.window_name,
                    &font_id,
                );
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
                Self::render_active_effects_content(ui, content, settings);
                None
            }
            WindowContent::Targets => Self::render_targets_content(app_core, ui),
            WindowContent::Players => Self::render_players_content(app_core, ui),
            WindowContent::Countdown(countdown) => {
                Self::render_countdown_content(app_core, ui, countdown, &settings);
                None
            }
            WindowContent::Indicator(indicator) => {
                Self::render_indicator_content(ui, &tab.id.title, indicator);
                None
            }
            WindowContent::InjuryDoll(doll) => {
                Self::render_injury_doll_grid(ui, &doll.injuries);
                None
            }
            WindowContent::Dashboard { indicators } => {
                Self::render_dashboard_content(ui, indicators);
                None
            }
            WindowContent::GS4Experience => {
                Self::render_gs4_experience_content(app_core, ui, &settings);
                None
            }
            WindowContent::Experience => {
                Self::render_dr_experience_content(app_core, ui);
                None
            }
            WindowContent::Encumbrance => {
                Self::render_encumbrance_content(app_core, ui, &settings);
                None
            }
            WindowContent::Betrayer => {
                Self::render_betrayer_content(app_core, ui, &settings);
                None
            }
            WindowContent::Perception(perception) => {
                Self::render_perception_content(ui, perception)
            }
            WindowContent::Items => Self::render_items_content(app_core, ui),
            WindowContent::Container { container_title } => {
                Self::render_container_content(app_core, ui, container_title, settings.wrap_text);
                None
            }
            WindowContent::Quickbar => Self::render_quickbar_content(app_core, ui),
            WindowContent::Performance => {
                Self::render_performance_content(app_core, ui);
                None
            }
            WindowContent::CommandInput { .. } => {
                ui.weak("Command input is docked at the bottom of the GUI.");
                None
            }
            WindowContent::Empty => {
                // Spacers reserve their area and draw nothing.
                ui.allocate_space(ui.available_size());
                None
            }
        }
    }
}

/// Per-window cache of estimated line heights driving text virtualization.
/// Keyed in egui temp data by scroll id; tracks the rendered slice (the last
/// `MAX_RENDERED_LINES` of the buffer) at a specific wrap width/generation.
#[derive(Default)]
pub(super) struct RowHeightCache {
    wrap_width: f32,
    font_id: egui::FontId,
    generation: u64,
    heights: Vec<f32>,
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

    #[test]
    fn countdown_remaining_fraction_keeps_sub_seconds() {
        assert_eq!(
            VellumGuiApp::countdown_remaining_seconds_f(110, 0, 105.5),
            4.5
        );
    }

    #[test]
    fn countdown_remaining_fraction_clamps_to_zero_when_elapsed() {
        assert_eq!(
            VellumGuiApp::countdown_remaining_seconds_f(100, 0, 150.0),
            0.0
        );
    }

    #[test]
    fn countdown_remaining_fraction_applies_server_offset() {
        // Server clock runs 5s ahead of local time.
        assert_eq!(
            VellumGuiApp::countdown_remaining_seconds_f(110, 5, 100.0),
            5.0
        );
    }

    #[test]
    fn split_search_runs_marks_exact_matches() {
        let runs = VellumGuiApp::split_search_runs("Some walls, some shelves", "some");
        assert_eq!(
            runs,
            vec![
                ("Some", true),
                (" walls, ", false),
                ("some", true),
                (" shelves", false),
            ]
        );
    }

    #[test]
    fn split_search_runs_no_match_returns_whole_text() {
        let runs = VellumGuiApp::split_search_runs("nothing here", "xyz");
        assert_eq!(runs, vec![("nothing here", false)]);
    }

    #[test]
    fn split_search_runs_adjacent_matches() {
        let runs = VellumGuiApp::split_search_runs("aaa", "a");
        assert_eq!(runs, vec![("a", true), ("a", true), ("a", true)]);
    }

    #[test]
    fn injury_level_color_distinguishes_injuries_from_scars() {
        use eframe::egui::Color32;
        assert_eq!(
            VellumGuiApp::injury_level_color(0),
            Color32::from_rgb(0x33, 0x33, 0x33)
        );
        assert_eq!(
            VellumGuiApp::injury_level_color(3),
            Color32::from_rgb(0xff, 0x00, 0x00)
        );
        assert_eq!(
            VellumGuiApp::injury_level_color(6),
            Color32::from_rgb(0x55, 0x55, 0x55)
        );
        // Out-of-range levels clamp to the deepest scar color.
        assert_eq!(
            VellumGuiApp::injury_level_color(9),
            VellumGuiApp::injury_level_color(6)
        );
    }
}
