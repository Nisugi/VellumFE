//! Styled-text rendering: segment-to-rich-text conversion, search-run
//! splitting, line jobs, buffer selection, line-height metrics, and the
//! scrollable text-window renderer itself.

use super::*;

/// How a floated image narrows and shifts one line of text.
///
/// The float is laid out by reserving a column on one side: the text wraps
/// to `width` instead of the full window, and (for a left float) paints
/// `x_offset` further right. A right float narrows without shifting, so
/// `x_offset` stays 0 and only `width` changes.
///
/// This is a single value computed ONCE per line and handed to every
/// consumer — measurement, painting, and the drag hit-test — because the
/// three must produce byte-identical galleys or selection lands on the wrong
/// character.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(in crate::frontend::gui::app) struct LineInset {
    /// Wrap width for this line (already reduced by the float).
    pub width: f32,
    /// How far right to paint the galley.
    pub x_offset: f32,
    /// How far DOWN to paint the galley.
    ///
    /// Non-zero only when a float collapsed to its own rows (too narrow to
    /// wrap beside): the text starts below the picture instead of over it.
    pub y_offset: f32,
    /// Painted height of the float's picture.
    ///
    /// Stored because the paint pass otherwise has to derive it from the
    /// row block's total stride — which stretches the picture whenever the
    /// text beside it is TALLER than the image (a narrow window wraps the
    /// origin row past the picture's bottom). The picture paints at its
    /// fitted size; the block just has to be big enough.
    pub float_height: f32,
    /// Width of the column the float reserves.
    ///
    /// Stored rather than re-derived at paint time: the painter's row width
    /// comes from the CURRENT layout, while `width` was computed when the
    /// row was measured. After a window resize those disagree, and deriving
    /// the image width from their difference collapsed the picture to a
    /// sliver behind the text.
    pub float_width: f32,
}

impl LineInset {
    /// No float: the line uses the full width and no shift.
    pub(in crate::frontend::gui::app) fn full(width: f32) -> Self {
        Self {
            width,
            x_offset: 0.0,
            y_offset: 0.0,
            float_height: 0.0,
            float_width: 0.0,
        }
    }
}

impl VellumGuiApp {
    /// Animate a bar fraction toward its target so server updates glide
    /// instead of jumping. The first paint for a given id snaps straight to
    /// the target, and egui keeps repainting while the value is moving, so
    /// this composes with repaint-on-demand at zero idle cost.
    pub(super) fn animated_fraction(ui: &egui::Ui, id_salt: &str, target: f32) -> f32 {
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
    pub(super) fn styled_rich_text(
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

    // Visibility: tested from app/tests.rs.
    pub(in crate::frontend::gui::app) fn segment_has_clickable_link(segment: &TextSegment) -> bool {
        // Parser may mark creature links as Monsterbold when links are wrapped in pushBold/popBold.
        // `link_data` is the reliable indicator of actual clickability.
        segment.link_data.is_some()
    }

    /// Allocation-free ASCII case-insensitive substring search starting at
    /// `from`. `needle_lower` must already be ASCII-lowercased. Byte indices
    /// returned are always char boundaries: a valid UTF-8 needle can never
    /// match starting on a continuation byte.
    pub(in crate::frontend::gui::app) fn find_ascii_ci(haystack: &str, needle_lower: &str, from: usize) -> Option<usize> {
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
    pub(super) fn segment_matches_query(segment: &TextSegment, query_lower: Option<&str>) -> bool {
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
    pub(super) fn segment_text_format(
        segment: &TextSegment,
        visuals: &egui::Visuals,
        search_match: bool,
        font_id: &egui::FontId,
    ) -> egui::TextFormat {
        Self::segment_text_format_ex(segment, visuals, search_match, false, font_id)
    }

    /// As segment_text_format, with the link fallback color when `is_link`.
    pub(super) fn segment_text_format_ex(
        segment: &TextSegment,
        visuals: &egui::Visuals,
        search_match: bool,
        is_link: bool,
        font_id: &egui::FontId,
    ) -> egui::TextFormat {
        let color = segment
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
    ///
    /// `custom_runs` are `(char_start, char_end, name)` slots within this job's
    /// text that hold a custom emoji's `:name:` fallback and must be overpainted
    /// with the emoji image. They are drained together with the job so the next
    /// flush starts clean.
    pub(super) fn flush_text_job(
        ui: &mut egui::Ui,
        job: &mut egui::text::LayoutJob,
        custom_runs: &mut Vec<(usize, usize, String)>,
    ) {
        if job.is_empty() {
            custom_runs.clear();
            return;
        }
        let job = std::mem::take(job);
        let runs = std::mem::take(custom_runs);
        if !runs.is_empty() || super::color_emoji::should_overlay(&job.text) {
            Self::add_label_with_color_emoji(ui, egui::Label::new(job), false, None, &runs);
        } else {
            ui.add(egui::Label::new(job));
        }
    }

    /// Paint custom-emoji images over the `:name:` fallback slots recorded for a
    /// galley. Mirrors `color_emoji::paint_color_emoji`: a pure overlay run
    /// after the galley is painted, so selection/copy still see the shortcode.
    /// A slot whose emoji fails to resolve is left as visible `:name:` text.
    pub(super) fn paint_custom_emoji_runs(
        ctx: &egui::Context,
        painter: &egui::Painter,
        galley: &egui::Galley,
        galley_pos: egui::Pos2,
        custom_runs: &[(usize, usize, String)],
    ) {
        // The placeholder is real spaces, so the cursor span is the true slot:
        // left edge at `start`, right edge at `end`. Center the emoji in it.
        for (start, end, name) in custom_runs {
            let start_rect = galley.pos_from_cursor(egui::text::CCursor::new(*start));
            let end_rect = galley.pos_from_cursor(egui::text::CCursor::new(*end));
            let slot = egui::Rect::from_min_max(
                galley_pos + start_rect.min.to_vec2(),
                galley_pos + end_rect.max.to_vec2(),
            );
            super::custom_emoji_render::paint_custom_emoji(ctx, painter, name, slot);
        }
    }

    /// Add a label whose text contains emoji, then paint color emoji
    /// textures over the monochrome glyphs.
    ///
    /// `Label::ui` never exposes its galley, so this path uses the public
    /// `Label::layout_in_ui` (identical layout, allocation, and response)
    /// and mirrors the paint block of `impl Widget for Label` from the egui
    /// fork (rev 426ef99, crates/egui/src/widgets/label.rs), minus the
    /// elided-text hover tooltip: our jobs never elide (no
    /// max_rows/truncate). Callers pass `interactive` = whether the label
    /// was given a non-hover sense, and the explicit `selectable` override
    /// if one was set on the label, matching what `Label::ui` derives.
    pub(super) fn add_label_with_color_emoji(
        ui: &mut egui::Ui,
        label: egui::Label,
        interactive: bool,
        selectable: Option<bool>,
        custom_runs: &[(usize, usize, String)],
    ) -> egui::Response {
        let (galley_pos, galley, response) = label.layout_in_ui(ui);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), galley.text())
        });
        if ui.is_rect_visible(response.rect) {
            let response_color = if interactive {
                ui.style().interact(&response).text_color()
            } else {
                ui.style().visuals.text_color()
            };
            let underline = if response.has_focus() || response.highlighted() {
                egui::Stroke::new(1.0, response_color)
            } else {
                egui::Stroke::NONE
            };
            let selectable =
                selectable.unwrap_or_else(|| ui.style().interaction.selectable_labels);
            if selectable {
                egui::text_selection::LabelSelectionState::label_text_selection(
                    ui,
                    &response,
                    galley_pos,
                    galley.clone(),
                    response_color,
                    underline,
                );
            } else {
                ui.painter().add(
                    egui::epaint::TextShape::new(galley_pos, galley.clone(), response_color)
                        .with_underline(underline),
                );
            }
            super::color_emoji::paint_color_emoji(ui.ctx(), ui.painter(), &galley, galley_pos);
            if !custom_runs.is_empty() {
                Self::paint_custom_emoji_runs(
                    ui.ctx(),
                    ui.painter(),
                    &galley,
                    galley_pos,
                    custom_runs,
                );
            }
        }
        response
    }

    /// Format a line's arrival time for display, matching the TUI's style
    /// (" [7:08 PM]" at end, "[7:08 PM] " at start).
    pub(super) fn format_line_timestamp(
        timestamp: i64,
        position: crate::config::TimestampPosition,
    ) -> Option<String> {
        use chrono::TimeZone;
        let local = chrono::Local.timestamp_opt(timestamp, 0).single()?;
        let time = local.format("%l:%M %p").to_string();
        let time = time.trim();
        Some(match position {
            crate::config::TimestampPosition::Start => format!("[{}] ", time),
            crate::config::TimestampPosition::End => format!(" [{}]", time),
        })
    }

    pub(super) fn render_styled_line(
        ui: &mut egui::Ui,
        line: &StyledLine,
        visuals: &egui::Visuals,
        search_query: Option<&str>,
        font_id: &egui::FontId,
        wrap: bool,
        timestamps: Option<crate::config::TimestampPosition>,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;
        // Pre-rendered timestamp run for this line, if enabled and stamped.
        let ts_run = timestamps.and_then(|position| {
            line.timestamp
                .and_then(|ts| Self::format_line_timestamp(ts, position))
                .map(|text| (text, position))
        });
        let ts_format = egui::text::TextFormat {
            font_id: font_id.clone(),
            color: visuals.weak_text_color(),
            ..Default::default()
        };

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
                // Custom-emoji `:name:` slots within the current job, as
                // `(char_start, char_end, name)`. `job_chars` tracks the char
                // count already appended so a slot's cursor range is known
                // before the fallback text goes in. Char (not byte) counts,
                // because galley cursors index by char.
                let mut custom_runs: Vec<(usize, usize, String)> = Vec::new();
                let mut job_chars = 0usize;

                if let Some((text, crate::config::TimestampPosition::Start)) = &ts_run {
                    job.append(text, 0.0, ts_format.clone());
                    job_chars += text.chars().count();
                }

                for segment in &line.segments {
                    if segment.text.is_empty() {
                        continue;
                    }

                    // Custom emoji: reserve the `:name:` fallback run and mark
                    // it for image overlay. Always render as an image (no
                    // monochrome fallback exists), independent of the
                    // color-emoji toggle. If the emoji can't resolve to an
                    // image, fall through to plain text so the slot shows
                    // `:name:` instead of a blank.
                    if let Some(name) = &segment.custom_emoji {
                        if super::custom_emoji_render::is_paintable(ui.ctx(), name) {
                            let start = job_chars;
                            let n = segment.text.chars().count();
                            job.append(
                                &segment.text,
                                0.0,
                                Self::segment_text_format(segment, visuals, false, font_id),
                            );
                            job_chars += n;
                            custom_runs.push((start, job_chars, name.clone()));
                            continue;
                        }
                        // Unresolved: fall through to the normal text paths.
                    }

                    let is_link = Self::segment_has_clickable_link(segment);
                    let search_match = Self::segment_matches_query(segment, search_query);

                    if is_link {
                        Self::flush_text_job(ui, &mut job, &mut custom_runs);
                        job_chars = 0;
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
                        let selectable = !Self::link_drag_blocks_selection(ui);
                        let label = egui::Label::new(rich)
                            .sense(egui::Sense::click_and_drag())
                            .selectable(selectable);
                        let response = if super::color_emoji::should_overlay(&segment.text) {
                            Self::add_label_with_color_emoji(ui, label, true, Some(selectable), &[])
                        } else {
                            ui.add(label)
                        }
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
                            job_chars += piece.chars().count();
                        }
                    } else {
                        job.append(
                            &segment.text,
                            0.0,
                            Self::segment_text_format(segment, visuals, false, font_id),
                        );
                        job_chars += segment.text.chars().count();
                    }
                }

                if let Some((text, crate::config::TimestampPosition::End)) = &ts_run {
                    job.append(text, 0.0, ts_format.clone());
                    job_chars += text.chars().count();
                }

                let _ = job_chars;
                Self::flush_text_job(ui, &mut job, &mut custom_runs);
                Self::line_tail_selection_filler(ui, font_id);
            };
            if wrap {
                ui.horizontal_wrapped(row);
            } else {
                ui.horizontal(row);
            }
        });

        clicked_link
    }

    /// Fill the blank remainder of a text row with an invisible selectable
    /// region. Pressing there anchors a text selection on that line (the
    /// empty galley contributes nothing to copied text) instead of falling
    /// through to the window body, which would drag the window around. On
    /// touch screens it stays drag-transparent so drag-to-scroll works.
    pub(super) fn line_tail_selection_filler(ui: &mut egui::Ui, font_id: &egui::FontId) {
        // The -1.0 keeps float rounding from pushing the filler onto the
        // next wrapped row.
        let width = ui.available_size_before_wrap().x - 1.0;
        if !width.is_finite() || width < 2.0 {
            return;
        }
        let height = ui.ctx().fonts_mut(|fonts| fonts.row_height(font_id));
        let sense = if ui.input(|i| i.has_touch_screen()) {
            egui::Sense::click()
        } else {
            egui::Sense::click_and_drag()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
        if !ui.is_rect_visible(rect) {
            return;
        }
        let galley = ui.ctx().fonts_mut(|fonts| {
            fonts.layout_job(egui::text::LayoutJob::simple_singleline(
                String::new(),
                font_id.clone(),
                Color32::TRANSPARENT,
            ))
        });
        egui::text_selection::LabelSelectionState::label_text_selection(
            ui,
            &response,
            rect.left_top(),
            galley,
            Color32::TRANSPARENT,
            egui::Stroke::NONE,
        );
    }

    /// One text line composed into a single layout job, with the char ranges
    /// (not bytes) of its clickable links. One galley per line keeps hit
    /// testing, selection painting, and height measurement all on the same
    /// geometry.
    /// The transparent space run a paintable custom emoji occupies: enough
    /// real spaces to cover the emoji square + padding (row_height *
    /// width_factor) at the current font. build_line_job and compose_line_text
    /// both call this so their char counts agree (copy/selection alignment).
    /// At least one space so the run always has width.
    pub(super) fn emoji_placeholder(ctx: &egui::Context, font_id: &egui::FontId) -> String {
        let row_h = ctx.fonts_mut(|f| f.row_height(font_id));
        let space_w = ctx.fonts_mut(|f| f.glyph_width(font_id, ' ')).max(1.0);
        let target_w = row_h * super::custom_emoji_render::width_factor();
        let n = (target_w / space_w).ceil().max(1.0) as usize;
        " ".repeat(n)
    }

    pub(super) fn build_line_job(
        ctx: &egui::Context,
        line: &StyledLine,
        visuals: &egui::Visuals,
        search_query: Option<&str>,
        font_id: &egui::FontId,
        inset: LineInset,
        timestamps: Option<crate::config::TimestampPosition>,
    ) -> GuiLineJob {
        let mut job = egui::text::LayoutJob {
            wrap: egui::text::TextWrapping {
                max_width: inset.width,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut links = Vec::new();
        let mut custom_runs: Vec<(usize, usize, String)> = Vec::new();
        let mut chars = 0usize;

        let ts_run = timestamps.and_then(|position| {
            line.timestamp
                .and_then(|ts| Self::format_line_timestamp(ts, position))
                .map(|text| (text, position))
        });
        let ts_format = egui::text::TextFormat {
            font_id: font_id.clone(),
            color: visuals.weak_text_color(),
            ..Default::default()
        };
        if let Some((text, crate::config::TimestampPosition::Start)) = &ts_run {
            chars += text.chars().count();
            job.append(text, 0.0, ts_format.clone());
        }

        for segment in &line.segments {
            if segment.text.is_empty() {
                continue;
            }
            let search_match = Self::segment_matches_query(segment, search_query);
            // Inline image: the picture is painted separately in its own
            // reserved column, so the `[img:name]` label must NOT also appear
            // as text beside it. It stays in the segment as the fallback for
            // frontends that cannot draw (the TUI) and for art that fails to
            // resolve — hence the paintable check, mirroring custom emoji.
            if let Some(image) = &segment.inline_image {
                if super::custom_emoji_render::inline_image_size(ctx, &image.name).is_some() {
                    continue;
                }
            }
            // Custom emoji: reserve the `:name:` fallback run and record it for
            // an image overlay painted after the galley. Only when it resolves
            // to a paintable image; otherwise fall through to plain text so the
            // `:name:` shows instead of a blank slot.
            if let Some(name) = &segment.custom_emoji {
                if super::custom_emoji_render::is_paintable(ctx, name) {
                    // Reserve a transparent run of N real spaces (not the wide
                    // `:name:` text) wide enough for the square emoji + padding.
                    // Real spaces advance the galley cursor predictably (unlike
                    // extra_letter_spacing, which the cursor API ignores), so
                    // the emoji can be centered in the true cursor span. The
                    // count must match compose_line_text so copy/selection
                    // offsets align; copy yields spaces, not `:name:`.
                    let placeholder = Self::emoji_placeholder(ctx, font_id);
                    let n = placeholder.chars().count();
                    let mut fmt =
                        Self::segment_text_format_ex(segment, visuals, false, false, font_id);
                    fmt.color = egui::Color32::TRANSPARENT;
                    job.append(&placeholder, 0.0, fmt);
                    custom_runs.push((chars, chars + n, name.clone()));
                    chars += n;
                    continue;
                }
            }
            if let Some(link_data) = &segment.link_data {
                // Links keep whole-segment search highlighting, matching the
                // old one-widget-per-link rendering.
                let count = segment.text.chars().count();
                links.push((chars..chars + count, link_data.clone()));
                chars += count;
                job.append(
                    &segment.text,
                    0.0,
                    Self::segment_text_format_ex(segment, visuals, search_match, true, font_id),
                );
            } else if search_match {
                let query = search_query.unwrap_or_default();
                for (piece, is_match) in Self::split_search_runs(&segment.text, query) {
                    chars += piece.chars().count();
                    job.append(
                        piece,
                        0.0,
                        Self::segment_text_format_ex(segment, visuals, is_match, false, font_id),
                    );
                }
            } else {
                chars += segment.text.chars().count();
                job.append(
                    &segment.text,
                    0.0,
                    Self::segment_text_format_ex(segment, visuals, false, false, font_id),
                );
            }
        }

        if let Some((text, crate::config::TimestampPosition::End)) = &ts_run {
            job.append(text, 0.0, ts_format);
        }

        // If the line carries a custom emoji rendered taller than the text
        // (size knob > 1), the row must grow so it isn't clipped by neighbors.
        let min_height = if custom_runs.is_empty() {
            0.0
        } else {
            let size = super::custom_emoji_render::size_factor();
            if size > 1.0 {
                ctx.fonts_mut(|f| f.row_height(font_id)) * size
            } else {
                0.0
            }
        };

        GuiLineJob {
            job,
            links,
            custom_runs,
            min_height,
        }
    }

    /// The plain text a line renders as (timestamps included when shown).
    /// Must compose the same string as build_line_job so char offsets from
    /// galley hit tests slice it correctly.
    pub(super) fn compose_line_text(
        ctx: &egui::Context,
        font_id: &egui::FontId,
        line: &StyledLine,
        timestamps: Option<crate::config::TimestampPosition>,
    ) -> String {
        let ts_run = timestamps.and_then(|position| {
            line.timestamp
                .and_then(|ts| Self::format_line_timestamp(ts, position))
                .map(|text| (text, position))
        });
        let mut out = String::new();
        if let Some((text, crate::config::TimestampPosition::Start)) = &ts_run {
            out.push_str(text);
        }
        for segment in &line.segments {
            // A paintable custom-emoji segment renders as the space placeholder
            // (see build_line_job), so it must compose as the SAME placeholder
            // here or copy/selection char offsets misalign. A non-paintable one
            // keeps its `:name:` text.
            if segment.custom_emoji.is_some()
                && super::custom_emoji_render::is_paintable(ctx, segment.custom_emoji.as_ref().unwrap())
            {
                out.push_str(&Self::emoji_placeholder(ctx, font_id));
            } else {
                out.push_str(&segment.text);
            }
        }
        if let Some((text, crate::config::TimestampPosition::End)) = &ts_run {
            out.push_str(text);
        }
        out
    }

    pub(super) fn buffer_selection_data_id() -> egui::Id {
        egui::Id::new("vellum_buffer_text_selection")
    }

    pub(super) fn buffer_selection(ctx: &egui::Context) -> Option<GuiBufferSelection> {
        ctx.data(|data| data.get_temp(Self::buffer_selection_data_id()))
    }

    /// True when a game-window buffer selection spans a non-empty range, i.e.
    /// the user has text highlighted that should own Copy/Cut over the command
    /// input. A collapsed selection (anchor == head) or none returns false.
    pub(super) fn active_buffer_selection_present(ctx: &egui::Context) -> bool {
        Self::buffer_selection(ctx).is_some_and(|sel| sel.anchor != sel.head)
    }

    pub(super) fn store_buffer_selection(ctx: &egui::Context, selection: Option<GuiBufferSelection>) {
        ctx.data_mut(|data| match selection {
            Some(selection) => {
                data.insert_temp(Self::buffer_selection_data_id(), selection);
            }
            None => {
                data.remove::<GuiBufferSelection>(Self::buffer_selection_data_id());
            }
        });
    }

    /// Resolve a line uid back to an index in the current buffer. Uids that
    /// were trimmed off the front clamp to the first line; anything past the
    /// end clamps to the last.
    pub(super) fn resolve_line_uid(base_uid: u64, len: usize, uid: u64) -> usize {
        let rel = uid.wrapping_sub(base_uid);
        if (rel as usize) < len && rel <= usize::MAX as u64 {
            rel as usize
        } else if rel > u64::MAX / 2 {
            0
        } else {
            len.saturating_sub(1)
        }
    }

    /// Selection endpoints as ordered (line index, char) pairs.
    pub(super) fn ordered_selection_endpoints(
        selection: &GuiBufferSelection,
        base_uid: u64,
        len: usize,
    ) -> ((usize, usize), (usize, usize)) {
        let a = (
            Self::resolve_line_uid(base_uid, len, selection.anchor.0),
            selection.anchor.1,
        );
        let h = (
            Self::resolve_line_uid(base_uid, len, selection.head.0),
            selection.head.1,
        );
        if a <= h { (a, h) } else { (h, a) }
    }

    /// Slice a line's text by char offsets (`None` = line start/end).
    pub(super) fn slice_line_by_chars(text: &str, from: Option<usize>, to: Option<usize>) -> &str {
        let char_to_byte = |c: usize| {
            text.char_indices()
                .nth(c)
                .map(|(byte, _)| byte)
                .unwrap_or(text.len())
        };
        let b0 = from.map(char_to_byte).unwrap_or(0);
        let b1 = to.map(char_to_byte).unwrap_or(text.len());
        &text[b0.min(b1)..b0.max(b1)]
    }

    /// Assemble the copy text for a selection, walking the buffer directly so
    /// lines outside the rendered viewport are included.
    pub(super) fn buffer_selection_copy_text(
        ctx: &egui::Context,
        font_id: &egui::FontId,
        content: &TextContent,
        selection: &GuiBufferSelection,
        base_uid: u64,
        timestamps: Option<crate::config::TimestampPosition>,
    ) -> String {
        let len = content.lines.len();
        if len == 0 {
            return String::new();
        }
        let ((l0, c0), (l1, c1)) =
            Self::ordered_selection_endpoints(selection, base_uid, len);
        let mut out = String::new();
        for index in l0..=l1 {
            let Some(line) = content.lines.get(index) else {
                continue;
            };
            let text = Self::compose_line_text(ctx, font_id, line, timestamps);
            let from = (index == l0).then_some(c0);
            let to = (index == l1).then_some(c1);
            if index > l0 {
                out.push('\n');
            }
            out.push_str(Self::slice_line_by_chars(&text, from, to));
        }
        out
    }

    /// Char range of the word around `at` for double-click selection.
    pub(super) fn word_char_range(text: &str, at: usize) -> (usize, usize) {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return (0, 0);
        }
        let at = at.min(chars.len() - 1);
        let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
        if !is_word(chars[at]) {
            return (at, at + 1);
        }
        let mut start = at;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let mut end = at + 1;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        (start, end)
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
        if !auto_contrast
            || crate::frontend::gui::app::theme::contrast_ratio(preferred, background) >= 3.0
        {
            return preferred;
        }
        if crate::frontend::gui::app::theme::relative_luminance(background) > 0.35 {
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
    pub(super) fn paint_split_galley(
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
    pub(super) fn measure_line_height(
        ctx: &egui::Context,
        line: &StyledLine,
        visuals: &egui::Visuals,
        inset: LineInset,
        font_id: &egui::FontId,
        timestamps: Option<crate::config::TimestampPosition>,
    ) -> f32 {
        // Same job builder as rendering, so measured heights match rendered
        // heights exactly (timestamps included).
        let built =
            Self::build_line_job(ctx, line, visuals, None, font_id, inset, timestamps);
        let min_height = built.min_height;
        if built.job.is_empty() {
            // Blank line: renders as one empty text row.
            return ctx.fonts_mut(|fonts| fonts.row_height(font_id)).max(min_height);
        }
        ctx.fonts_mut(|fonts| fonts.layout_job(built.job))
            .size()
            .y
            .max(min_height)
    }

    /// Bring the height cache in sync with the rendered slice
    /// `content.lines[start..start + rendered_count]`. Appends measure only
    /// the new lines; width changes or non-monotonic generations rebuild.
    ///
    /// The scroll-anchoring pre-pass in `render_text_content` reads the
    /// heights this update is about to drain, so it must run before this.
    /// Assign float geometry to the cached rows: which row originates each
    /// image, how wide the text beside it wraps, and how many rows it covers.
    ///
    /// Mirrors the room window's proven layout (`boards.rs::float_covered_end`)
    /// but runs during MEASUREMENT rather than paint, because virtualization
    /// has to know a row is floated before it decides whether to lay it out.
    ///
    /// Rows are taken greedily while they fit beside the image. A row that
    /// would straddle the image's bottom edge is excluded and rejoins the
    /// full width — egui's single `max_width` per job cannot shorten part of
    /// a row, so excluding the straddler is the honest approximation (the
    /// same rule the room window documents).
    #[allow(clippy::too_many_arguments)]
    fn layout_floats(
        cache: &mut RowHeightCache,
        ctx: &egui::Context,
        content: &TextContent,
        start: usize,
        wrap_width: f32,
        view_height: f32,
        visuals: &egui::Visuals,
        font_id: &egui::FontId,
        timestamps: Option<crate::config::TimestampPosition>,
    ) {
        if !wrap_width.is_finite() || wrap_width <= 0.0 {
            return; // horizontal-scroll windows never float
        }
        let row_height = ctx.fonts_mut(|f| f.row_height(font_id)).max(1.0);

        let mut i = 0usize;
        while i < cache.heights.len() {
            let Some(line) = content.lines.get(start + i) else {
                break;
            };
            let Some(image) = line.segments.iter().find_map(|s| s.inline_image.as_ref())
            else {
                i += 1;
                continue;
            };
            let Some(natural) = super::custom_emoji_render::inline_image_size(ctx, &image.name)
            else {
                i += 1; // art not installed: the `[img:name]` text stands in
                continue;
            };

            let (img_w, img_h) = image.fitted_size(
                (natural.x, natural.y),
                row_height,
                wrap_width,
                view_height,
                crate::data::INLINE_IMAGE_MAX_ROWS,
            );
            if crate::data::InlineImage::should_collapse(img_w, wrap_width) {
                // Too narrow to wrap beside: the image takes its OWN rows and
                // the text starts below it.
                //
                // The row must reserve the picture's full height, and the
                // line's own text must be pushed past it — leaving the inset
                // at full width made the text wrap across the whole row and
                // paint on top of the picture, which is what a narrow window
                // actually showed.
                cache.extra[i] = img_h;
                cache.spans[i] = 1;
                cache.insets[i] = LineInset {
                    width: wrap_width,
                    x_offset: 0.0,
                    y_offset: img_h,
                    float_height: img_h,
                    float_width: img_w,
                };
                i += 1;
                continue;
            }

            let text_w = (wrap_width - img_w).max(1.0);
            let x_offset = match image.align {
                crate::data::FloatAlign::Left => img_w,
                crate::data::FloatAlign::Right => 0.0,
            };
            let inset = LineInset {
                width: text_w,
                x_offset,
                y_offset: 0.0,
                // The FITTED height, captured before the origin-row growth
                // below: the picture paints at this size even when the text
                // block beside it ends up taller.
                float_height: img_h,
                float_width: img_w,
            };

            // Re-measure the covered rows at the narrower width, taking them
            // while they fit within the image's height.
            // If the ORIGIN row's own text needs more height than the
            // picture covers, the picture cannot "end" partway down a single
            // galley — egui wraps a line to one width for its whole height.
            // Grow the reserved column to that row instead, so the text
            // stays beside the picture rather than running across it.
            let origin_h = Self::measure_line_height(
                ctx,
                &content.lines[start + i],
                visuals,
                inset,
                font_id,
                timestamps,
            );
            let img_h = img_h.max(origin_h);

            let mut used = 0.0f32;
            let mut span = 0usize;
            while i + span < cache.heights.len() {
                let Some(covered) = content.lines.get(start + i + span) else {
                    break;
                };
                // A row carrying its own image ends this float rather than
                // nesting: stacked floats are out of scope, and a hard break
                // beats overlapping pictures.
                if span > 0 && covered.segments.iter().any(|s| s.inline_image.is_some()) {
                    break;
                }
                let h = Self::measure_line_height(
                    ctx, covered, visuals, inset, font_id, timestamps,
                );
                if span > 0 && used + h > img_h {
                    break; // would straddle the image's bottom edge
                }
                cache.heights[i + span] = h;
                cache.insets[i + span] = inset;
                used += h;
                span += 1;
            }

            // Reserve any image height the covered text did not consume, so
            // the row block is tall enough for the picture.
            //
            // `used` can EXCEED img_h: the origin row is always taken (it is
            // the line carrying the image), and in a narrow window its own
            // text can wrap to more rows than the picture is tall. That is
            // fine — extra goes to zero and the block is text-height — but
            // the rows past the image must go back to FULL width, or they
            // keep the narrowed inset and wrap into the picture's column.
            let last = i + span.saturating_sub(1);
            cache.extra[last] = (img_h - used).max(0.0);

            // Any covered row whose top starts below the image rejoins the
            // full width. Walk the span accumulating heights; once past
            // img_h, the float no longer applies to that row.
            let mut y = 0.0f32;
            for offset in 0..span {
                let row = i + offset;
                if y >= img_h {
                    cache.insets[row] = LineInset::full(wrap_width);
                    cache.heights[row] = Self::measure_line_height(
                        ctx,
                        &content.lines[start + row],
                        visuals,
                        cache.insets[row],
                        font_id,
                        timestamps,
                    );
                }
                y += cache.heights[row];
            }
            cache.spans[i] = span.min(u16::MAX as usize) as u16;
            i += span.max(1);
        }
    }

    pub(super) fn update_row_height_cache(
        cache: &mut RowHeightCache,
        ctx: &egui::Context,
        content: &TextContent,
        start: usize,
        rendered_count: usize,
        wrap_width: f32,
        visuals: &egui::Visuals,
        font_id: &egui::FontId,
        float_epoch: u64,
        view_height: f32,
    ) {
        let timestamps = content.show_timestamps.then_some(content.timestamp_position);
        // `float_epoch` rides the same "rebuild everything" test as width and
        // font: a float's height depends on the window's own row count and on
        // art that may resolve asynchronously, neither of which changes
        // wrap_width or the generation. Without this the incremental path
        // would never re-measure the affected rows.
        let width_changed = (cache.wrap_width - wrap_width).abs() > 0.5
            || cache.font_id != *font_id
            || cache.float_epoch != float_epoch;
        let delta = content.generation.wrapping_sub(cache.generation) as usize;
        // A newly appended line carrying an image needs the FULL layout pass:
        // a float's span depends on the rows after it, which the incremental
        // path (append-only) cannot compute. `float_epoch` does not catch
        // this — it tracks the window's row capacity, which an arriving line
        // does not change — so test the new lines themselves.
        let appended_float = delta > 0
            && delta <= content.lines.len()
            && content
                .lines
                .iter()
                .skip(content.lines.len() - delta)
                .any(|line| line.segments.iter().any(|s| s.inline_image.is_some()));
        let incremental = !width_changed
            && !appended_float
            && content.generation >= cache.generation
            && delta <= rendered_count
            && cache.heights.len() + delta >= rendered_count;

        if incremental {
            if delta > 0 {
                let drop_front = (cache.heights.len() + delta).saturating_sub(rendered_count);
                cache.heights.drain(..drop_front.min(cache.heights.len()));
                // `extra` and `insets` are parallel to `heights` and must
                // slide with it, or reservations and wrap widths would drift
                // onto the wrong rows as the ring buffer trims.
                cache.extra.drain(..drop_front.min(cache.extra.len()));
                cache.insets.drain(..drop_front.min(cache.insets.len()));
                cache.spans.drain(..drop_front.min(cache.spans.len()));
                let len = content.lines.len();
                for line in content.lines.iter().skip(len - delta) {
                    // Appended lines are past any float that began earlier in
                    // the buffer, so they lay out at full width. A float that
                    // starts ON one of these lines is picked up by the next
                    // full rebuild (its epoch bumps when the art resolves).
                    let inset = LineInset::full(wrap_width);
                    cache.heights.push(Self::measure_line_height(
                        ctx, line, visuals, inset, font_id, timestamps,
                    ));
                    cache.extra.push(0.0);
                    cache.insets.push(inset);
                    cache.spans.push(0);
                }
            }
        } else {
            cache.heights.clear();
            cache.extra.clear();
            cache.insets.clear();
            cache.spans.clear();
            cache.heights.reserve(rendered_count);
            cache.extra.reserve(rendered_count);
            cache.insets.reserve(rendered_count);
            for line in content.lines.iter().skip(start) {
                let inset = LineInset::full(wrap_width);
                cache.heights.push(Self::measure_line_height(
                    ctx, line, visuals, inset, font_id, timestamps,
                ));
                cache.extra.push(0.0);
                cache.insets.push(inset);
                cache.spans.push(0);
            }
        }
        // ---- Float layout ------------------------------------------------
        // Runs only on a full rebuild: a float's span depends on the heights
        // of the rows AFTER it, which the incremental append path does not
        // have (it only ever adds rows past the end). Anything that changes
        // float geometry — including a newly arrived image line — forces the
        // incremental path off, so this is not a gap.
        if !incremental {
            Self::layout_floats(
                cache,
                ctx,
                content,
                start,
                wrap_width,
                view_height,
                visuals,
                font_id,
                timestamps,
            );
        }

        cache.wrap_width = wrap_width;
        cache.font_id = font_id.clone();
        cache.generation = content.generation;
        cache.float_epoch = float_epoch;
        debug_assert_eq!(cache.heights.len(), rendered_count);
        debug_assert_eq!(
            cache.extra.len(),
            cache.heights.len(),
            "extra must stay parallel to heights"
        );
        debug_assert_eq!(
            cache.insets.len(),
            cache.heights.len(),
            "insets must stay parallel to heights"
        );
        debug_assert_eq!(
            cache.spans.len(),
            cache.heights.len(),
            "spans must stay parallel to heights"
        );
    }

    pub(super) fn render_text_content(
        ui: &mut egui::Ui,
        content: &TextContent,
        scroll_id: &str,
        search_query: Option<&str>,
        font_id: &egui::FontId,
        wrap: bool,
        content_align: Option<&str>,
    ) -> Option<GuiLinkClick> {
        // content_align (shared layout def, long honored by the TUI): the
        // horizontal component offsets each line's galley; the vertical
        // component pads above the block while the whole buffer is shorter
        // than the viewport. Once content overflows, scrolling is unchanged.
        use crate::config::ContentAlign;
        let align = content_align.map(ContentAlign::from_str);
        let h_align: u8 = match align {
            Some(ContentAlign::Top | ContentAlign::Center | ContentAlign::Bottom) => 1,
            Some(
                ContentAlign::TopRight | ContentAlign::Right | ContentAlign::BottomRight,
            ) => 2,
            _ => 0,
        };
        let v_align: u8 = match align {
            Some(ContentAlign::Left | ContentAlign::Center | ContentAlign::Right) => 1,
            Some(
                ContentAlign::BottomLeft | ContentAlign::Bottom | ContentAlign::BottomRight,
            ) => 2,
            _ => 0,
        };
        // Cheap Arc clone; deep-cloning Visuals per window per frame is not.
        let style = ui.style().clone();
        let visuals = &style.visuals;
        let mut clicked_link = None;
        let rendered_count = content.lines.len().min(MAX_RENDERED_LINES);
        let start = content.lines.len() - rendered_count;
        let max_height = ui.available_height().max(1.0);
        let cache_id = egui::Id::new(("text_row_heights", scroll_id));

        // ---- Same-frame scroll anchoring ---------------------------------
        // Once the ring buffer is full, each appended line drops one off the
        // front and every remaining row shifts up, while the persisted
        // scroll offset stays a raw pixel value. Nudge the stored offset by
        // the outgoing rows' strides (known from LAST frame's height cache)
        // BEFORE the ScrollArea reads it, so an up-scrolled reader keeps
        // their exact place with no one-frame flicker. While following the
        // tail this is a no-op: the offset is pinned to the end below
        // regardless of the stored value. The area id comes from last
        // frame's ScrollAreaOutput (stashed below) rather than re-deriving
        // egui's salt hashing.
        let outer_ctx = ui.ctx().clone();
        let outer_spacing_y = ui.spacing().item_spacing.y;
        let area_id_key = egui::Id::new(("text_scroll_area_id", scroll_id));
        let cache_handle = outer_ctx.data_mut(|data| {
            data.get_temp_mut_or_insert_with::<std::sync::Arc<
                std::sync::Mutex<RowHeightCache>,
            >>(cache_id, Default::default)
                .clone()
        });
        {
            let cache = cache_handle.lock().expect("row height cache poisoned");
            let delta = content.generation.wrapping_sub(cache.generation) as usize;
            // Mirrors update_row_height_cache's incremental test, minus the
            // wrap-width check (unknown until layout runs); a width change
            // means a reflow that scrambles positions anyway.
            let incremental = content.generation >= cache.generation
                && delta <= rendered_count
                && cache.heights.len() + delta >= rendered_count;
            if incremental && delta > 0 {
                let drop_front = (cache.heights.len() + delta)
                    .saturating_sub(rendered_count)
                    .min(cache.heights.len());
                if drop_front > 0 {
                    let dropped_px: f32 = cache.stride_sum(0..drop_front, outer_spacing_y);
                    let area_id =
                        outer_ctx.data_mut(|data| data.get_temp::<egui::Id>(area_id_key));
                    if let Some(area_id) = area_id {
                        if let Some(mut state) =
                            egui::scroll_area::State::load(&outer_ctx, area_id)
                        {
                            state.offset.y = (state.offset.y - dropped_px).max(0.0);
                            state.store(&outer_ctx, area_id);
                        }
                    }
                }
            }
        }

        // Viewport height for keyboard/controller paging (see
        // try_gui_scroll_action) — refreshed every frame.
        outer_ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new(("text_scroll_view_h", scroll_id)),
                max_height,
            );
        });

        // Float geometry epoch: bumped whenever anything that changes a
        // float's size changes. Float heights depend on the window's own row
        // count (and on art that resolves asynchronously), neither of which
        // moves wrap_width or the buffer generation — so without this the
        // incremental cache path would never re-measure the affected rows.
        // P2.2 folds resolved image sizes in; today only the window's row
        // capacity varies, which is exactly the resize case.
        let float_epoch: u64 = {
            let row_h = ui.ctx().fonts_mut(|f| f.row_height(font_id)).max(1.0);
            (max_height / row_h).floor().max(0.0) as u64
        };

        let mut scroll_area = if wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };

        // Scroll position has exactly ONE authority: `follow_bottom`.
        //
        // egui's `stick_to_bottom` cannot be suspended — its stuck flag is
        // private with no setter, and it re-sticks only on exact float
        // equality — so negotiating with it required a hold loop that
        // re-asserted an offset every frame, a settle/clamp, and a snap to
        // re-arm stickiness. Those three mechanisms were a second, competing
        // source of truth, and they produced two real bugs: trim
        // compensation wrote to an offset the hold then discarded, and a
        // level-triggered producer (the phantom gamepad axis, 82c2a8d5)
        // could rebuild the hold faster than user input cleared it.
        //
        // Instead we do not use stick_to_bottom at all: while following, we
        // set the offset to the end ourselves each frame. "Following" is a
        // single persisted bool that user input clears idempotently, so a
        // repeating producer can no longer starve the mouse.
        let pending_key = egui::Id::new(("text_scroll_pending", scroll_id));
        let follow_key = egui::Id::new(("text_scroll_follow", scroll_id));
        let pending: Option<(u8, f32)> = outer_ctx.data_mut(|data| {
            let value = data.get_temp(pending_key);
            if value.is_some() {
                data.remove::<(u8, f32)>(pending_key);
            }
            value
        });
        // Default to following: a fresh window shows the newest text.
        let mut follow_bottom: bool = outer_ctx
            .data_mut(|data| data.get_temp(follow_key))
            .unwrap_or(true);
        // An explicit offset to apply this frame (a programmatic jump).
        // None means "leave the offset alone" — either we are following (and
        // pin to the end below) or the user owns it.
        let mut goto: Option<f32> = None;

        // The user's own scroll input takes over instantly.
        // A wheel event is not a single-frame signal: egui SMOOTHS it, so the
        // motion it produces lands over several later frames that carry no
        // raw event of their own. Pinning the offset on any of those frames
        // would swallow the rest of the gesture, so the smoothed delta counts
        // as user input for as long as it is still moving.
        let user_scrolled = ui.input(|input| {
            input.smooth_scroll_delta.y != 0.0
                || input.raw.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::MouseWheel { .. }
                            | egui::Event::PointerButton { pressed: true, .. }
                    )
                })
        });
        // User input owns the window the moment it arrives. Setting the flag
        // is idempotent, so a producer repeating every frame can no longer
        // out-race it (the 82c2a8d5 failure: "the mouse lost every round").
        if user_scrolled {
            follow_bottom = false;
        }

        if let Some((kind, value)) = pending {
            let current = outer_ctx
                .data_mut(|data| data.get_temp::<egui::Id>(area_id_key))
                .and_then(|area_id| egui::scroll_area::State::load(&outer_ctx, area_id))
                .map(|state| state.offset.y)
                .unwrap_or(0.0);
            match kind {
                1 => {
                    // Home
                    follow_bottom = false;
                    goto = Some(0.0);
                }
                2 => {
                    // End: resume following the tail.
                    follow_bottom = true;
                }
                3 => {
                    // Absolute: scroll so buffer line `value` sits near the
                    // top. The height cache covers only the rendered tail
                    // (lines `start..`), so map the absolute index into it
                    // and sum the preceding rows' strides.
                    let target_line = value as usize;
                    follow_bottom = false;
                    goto = if target_line < start {
                        Some(0.0)
                    } else {
                        let rendered_idx = target_line - start;
                        let cache = cache_handle.lock().expect("row height cache poisoned");
                        // Past the cached tail: clamp to the last cached row
                        // rather than silently becoming "jump to the end" —
                        // a search hit off the top of the buffer should land
                        // as close as we can get, not at the newest line.
                        let idx = rendered_idx.min(cache.heights.len().saturating_sub(1));
                        Some(cache.stride_sum(0..idx, outer_spacing_y).max(0.0))
                    };
                }
                _ => {
                    // Relative nudge (page/line keys, controller stick).
                    follow_bottom = false;
                    goto = Some((current + value).max(0.0));
                }
            }
        }

        // Following pins to the end; egui's own stickiness stays off so
        // there is never a second authority to negotiate with.
        //
        // The target is the content height we already know from the height
        // cache (the same sum the bottom spacer uses), NOT a huge sentinel:
        // without stick_to_bottom nothing clamps an out-of-range offset, so
        // a sentinel would be stored verbatim and the window would render
        // blank. Any error here self-corrects — the post-pass below re-pins
        // against the real layout on the very next frame.
        //
        // Skipped on a frame carrying user scroll input: an explicit offset
        // overrides egui's own wheel/drag handling, so pinning here would
        // swallow the very gesture that is supposed to take the window back.
        // `user_scrolled` already cleared `follow_bottom` above; this guard
        // covers the same frame's pin.
        // The builder offset is applied BEFORE the pass and overwrites the
        // stored value (egui scroll_area.rs:743), so pinning unconditionally
        // would clobber the wheel/drag every frame and the window could never
        // be scrolled by hand. Pin only when the stored position is actually
        // behind the tail — i.e. new content arrived — and never on a frame
        // carrying user scroll input.
        if follow_bottom && !user_scrolled {
            let content_h: f32 = {
                let cache = cache_handle.lock().expect("row height cache poisoned");
                cache.stride_sum(0..cache.heights.len(), outer_spacing_y)
            };
            let target = (content_h - max_height).max(0.0);
            let stored = outer_ctx
                .data_mut(|data| data.get_temp::<egui::Id>(area_id_key))
                .and_then(|area_id| egui::scroll_area::State::load(&outer_ctx, area_id))
                .map(|state| state.offset.y);
            if stored.is_none_or(|current| target - current > 0.5) {
                goto = Some(target);
            }
        }
        if let Some(target) = goto {
            scroll_area = scroll_area.vertical_scroll_offset(target);
        }

        let output = scroll_area
            .id_salt(format!("text_scroll_{}", scroll_id))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show_viewport(ui, |ui, viewport| {
                let is_touch = ui.input(|i| i.has_touch_screen());
                // Drags on blank space between/below lines deliberately
                // fall through to the window body: windows drag from
                // anywhere now, and blank space is how a text window is
                // moved without its title bar. Drags starting ON text stay
                // with the line widgets (selection), and Lock Window is
                // the guard against accidental moves.
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
                let timestamps = content.show_timestamps.then_some(content.timestamp_position);
                let base_uid = content
                    .generation
                    .wrapping_sub(content.lines.len() as u64);
                // Vertical alignment pad, from last frame's height cache (it
                // settles within a frame). Applied before content_top is read
                // so all selection/viewport math stays consistent.
                if v_align != 0 {
                    let cache = cache_handle.lock().expect("row height cache poisoned");
                    if cache.heights.len() == rendered_count {
                        let total: f32 =
                            cache.stride_sum(0..cache.heights.len(), spacing_y);
                        let free = max_height - total;
                        if free > 0.0 {
                            ui.add_space(if v_align == 1 { free / 2.0 } else { free });
                        }
                    }
                }
                // Top of line 0 in ui coords; the height cache turns this
                // into every line's y-band, on or off screen.
                let content_left = ui.max_rect().left();
                let content_top = ui.cursor().min.y;

                // The cache lives in egui temp data (fetched before the
                // scroll area) so renderers stay stateless; the Arc dance
                // keeps ctx.fonts_mut() callable while the cache is borrowed
                // (calling it inside ctx.data_mut would deadlock on the
                // context lock).
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
                    float_epoch,
                    max_height,
                );

                // ---- Buffer-anchored selection: window-level updates ----
                let clip = ui.clip_rect();
                let mut selection = Self::buffer_selection(&ctx);
                let pointer = ctx.pointer_latest_pos();
                let press_pos = ui.input(|i| i.pointer.interact_pos());
                let primary_down =
                    ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
                let any_pressed = ui.input(|i| i.pointer.any_pressed());
                let owns_selection = selection
                    .as_ref()
                    .is_some_and(|sel| sel.scroll_id == scroll_id);

                // Pressing outside this window, or Escape, drops our selection.
                if owns_selection {
                    let pressed_outside = any_pressed
                        && !press_pos.is_some_and(|pos| clip.contains(pos));
                    if pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        selection = None;
                        Self::store_buffer_selection(&ctx, None);
                    }
                }

                // Continue a selection drag: the height cache maps the
                // pointer to a line even past the viewport edges, and the
                // view auto-scrolls toward the pointer while it is outside.
                if let Some(sel) = &mut selection {
                    if sel.scroll_id == scroll_id && sel.dragging {
                        if !primary_down {
                            sel.dragging = false;
                            Self::store_buffer_selection(&ctx, selection.clone());
                        } else if let Some(pos) = pointer {
                            let mut slot = rendered_count - 1;
                            let mut slot_top = content_top;
                            let mut y = content_top;
                            for i in 0..cache.heights.len() {
                                let h = cache.stride(i);
                                if pos.y < y + h + spacing_y || i == rendered_count - 1 {
                                    slot = i;
                                    slot_top = y;
                                    break;
                                }
                                y += h + spacing_y;
                            }
                            let line_index = start + slot;
                            // Reuse the row's MEASURED layout: laying it out
                            // at a different width here would produce a
                            // different galley, and the character the drag
                            // resolves to would not be the one painted.
                            let line_job = Self::build_line_job(
                                &ctx,
                                &content.lines[line_index],
                                &visuals,
                                search_query,
                                font_id,
                                cache.inset(slot, wrap_width),
                                timestamps,
                            );
                            let galley = ctx.fonts_mut(|fonts| fonts.layout_job(line_job.job));
                            // Centered/right rows paint their galley offset
                            // within the full-width row; mirror that offset
                            // when mapping the pointer back to a character.
                            let drag_inset = cache.inset(slot, wrap_width);
                            let drag_h_offset = if h_align != 0 && drag_inset.width.is_finite() {
                                let free = (drag_inset.width - galley.size().x).max(0.0);
                                if h_align == 1 { free / 2.0 } else { free }
                            } else {
                                0.0
                            };
                            // Subtract the float's column too: the galley was
                            // PAINTED that far right, so the pointer must be
                            // mapped back through the same shift or the drag
                            // selects the wrong character.
                            let local = egui::Vec2::new(
                                pos.x - content_left - drag_h_offset - drag_inset.x_offset,
                                // A collapsed float paints its galley BELOW
                                // the picture; map the pointer through the
                                // same shift or a drag in that block selects
                                // rows-of-picture instead of text.
                                pos.y - slot_top - drag_inset.y_offset,
                            );
                            sel.head = (
                                base_uid.wrapping_add(line_index as u64),
                                galley.cursor_from_pos(local).index.0,
                            );
                            Self::store_buffer_selection(&ctx, selection.clone());
                            ctx.set_cursor_icon(egui::CursorIcon::Text);

                            let overshoot_up = clip.top() - pos.y;
                            let overshoot_down = pos.y - clip.bottom();
                            let overshoot = overshoot_up.max(overshoot_down);
                            if overshoot > 0.0 {
                                let speed = (overshoot * 0.3).clamp(2.0, 40.0);
                                let direction = if overshoot_up > 0.0 { 1.0 } else { -1.0 };
                                ui.scroll_with_delta(Vec2::new(0.0, direction * speed));
                                ctx.request_repaint();
                            }
                        }
                    }
                }

                // Ctrl+C / Ctrl+X copy the selected range straight from the
                // buffer, so lines scrolled out of view are included.
                if ui.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut))
                }) {
                    if let Some(sel) = &selection {
                        if sel.scroll_id == scroll_id && sel.anchor != sel.head {
                            let text = Self::buffer_selection_copy_text(
                                &ctx, font_id, content, sel, base_uid, timestamps,
                            );
                            if !text.is_empty() {
                                ctx.copy_text(text);
                            }
                            // Consume the event so a command input rendering
                            // later this frame can't also claim the clipboard
                            // (bug #3). The command-input widget makes the same
                            // check up front for the reverse order.
                            ctx.input_mut(|input| {
                                input.events.retain(|event| {
                                    !matches!(event, egui::Event::Copy | egui::Event::Cut)
                                });
                            });
                        }
                    }
                }

                // Ordered (line index, char) endpoints for highlight painting.
                let paint_range = selection
                    .as_ref()
                    .filter(|sel| sel.scroll_id == scroll_id && sel.anchor != sel.head)
                    .map(|sel| {
                        Self::ordered_selection_endpoints(sel, base_uid, content.lines.len())
                    });

                // Visible index range from cumulative strides (height +
                // vertical item spacing). Only those lines become widgets;
                // the rest are stand-in spacers.
                let top = viewport.min.y.max(0.0);
                let bottom = viewport.max.y.max(top);
                let mut first_visible = rendered_count;
                let mut top_space = 0.0f32;
                let mut y = 0.0f32;
                for i in 0..cache.heights.len() {
                    let stride = cache.stride(i) + spacing_y;
                    if y + stride > top {
                        first_visible = i;
                        top_space = y;
                        break;
                    }
                    y += stride;
                }
                // A float that STARTED above the viewport still overhangs
                // into it, and only its origin row paints the image. Walk
                // back to that origin (and reclaim the space we already
                // counted) so scrolling into the middle of a float does not
                // make the picture vanish.
                if first_visible < rendered_count {
                    let origin = cache.float_origin_at(first_visible);
                    if origin < first_visible {
                        top_space -= cache.stride_sum(origin..first_visible, spacing_y);
                        first_visible = origin;
                    }
                }
                let mut last_visible = rendered_count;
                let mut yy = top_space;
                for i in first_visible..rendered_count {
                    if yy > bottom {
                        last_visible = i;
                        break;
                    }
                    yy += cache.stride(i) + spacing_y;
                }

                if first_visible > 0 && top_space > spacing_y {
                    // The spacer's trailing item_spacing stands in for the
                    // last skipped line's own spacing.
                    ui.allocate_space(Vec2::new(1.0, top_space - spacing_y));
                }
                let mut press_claimed_by_line = false;
                for (offset, line) in content
                    .lines
                    .iter()
                    .skip(start + first_visible)
                    .take(last_visible.saturating_sub(first_visible))
                    .enumerate()
                {
                    let slot = first_visible + offset;
                    let line_index = start + slot;
                    let uid = base_uid.wrapping_add(line_index as u64);

                    let line_inset = cache.inset(slot, wrap_width);
                    let line_job = Self::build_line_job(
                        &ctx,
                        line,
                        &visuals,
                        search_query,
                        font_id,
                        line_inset,
                        timestamps,
                    );
                    let links = line_job.links;
                    let custom_runs = line_job.custom_runs;
                    let emoji_min_height = line_job.min_height;
                    let mut galley = ctx.fonts_mut(|fonts| fonts.layout_job(line_job.job));
                    let galley_size = galley.size();
                    // Grow the row for an oversized emoji so it isn't clipped
                    // (must match measure_line_height's .max(min_height)).
                    let height = if galley_size.y > 0.0 {
                        galley_size.y
                    } else {
                        ctx.fonts_mut(|fonts| fonts.row_height(font_id))
                    }
                    .max(emoji_min_height);
                    // Full-width rows: the blank tail past the text belongs
                    // to the line, so clicks there select from that line and
                    // never fall through to the window body.
                    let width = if wrap {
                        ui.available_width().max(1.0)
                    } else {
                        galley_size.x.max(ui.available_width().max(1.0))
                    };
                    let sense = if is_touch {
                        egui::Sense::click()
                    } else {
                        egui::Sense::click_and_drag()
                    };
                    // The row's rect must include the RESERVED float space
                    // (`extra`), not just the text height. The spacer math
                    // already counts extra via stride(); allocating only the
                    // galley here let every following line render straight
                    // over the bottom of the picture whenever the text
                    // beside it was shorter — the live "text overlaps the
                    // image" bug. The collapse case's y_offset rides inside
                    // extra, so it is covered by the same allocation.
                    let alloc_h = height + cache.extra.get(slot).copied().unwrap_or(0.0);
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, alloc_h), sense);
                    let h_offset = match h_align {
                        1 => ((rect.width() - galley_size.x) / 2.0).max(0.0),
                        2 => (rect.width() - galley_size.x).max(0.0),
                        _ => 0.0,
                    };
                    // The float's reserved column shifts the text right (a
                    // left float); h_align then centres/right-aligns within
                    // what remains. The hit-test below derives from this same
                    // galley_pos, so the two stay in agreement.
                    let galley_pos = rect.left_top()
                        + Vec2::new(h_offset + line_inset.x_offset, line_inset.y_offset);

                    // Paint the float this row originates. The image spans
                    // the rows the layout pass reserved, so its height comes
                    // from those strides rather than this row alone.
                    if cache.spans[slot] > 0 {
                        if let Some(image) =
                            line.segments.iter().find_map(|s| s.inline_image.as_ref())
                        {
                            let img_h = line_inset.float_height;
                            let img_w = line_inset.float_width;
                            if img_w > 0.0 && img_h > 0.0 {
                                let left = match image.align {
                                    crate::data::FloatAlign::Left => rect.left(),
                                    crate::data::FloatAlign::Right => rect.right() - img_w,
                                };
                                let img_rect = egui::Rect::from_min_size(
                                    egui::pos2(left, rect.top()),
                                    Vec2::new(img_w, img_h),
                                );
                                super::custom_emoji_render::paint_inline_image(
                                    &ctx,
                                    ui.painter(),
                                    &image.name,
                                    img_rect,
                                );
                                // Press-and-hold blows the picture up, the
                                // same gesture the room window offers.
                                //
                                // The ROW already owns this area's
                                // interaction (it was allocated with
                                // click_and_drag for text selection), so a
                                // second `interact` over the same rect would
                                // lose. Test the pointer directly instead:
                                // held down, and inside the picture.
                                let holding_image = ui.input(|i| {
                                    i.pointer.any_down()
                                        && i.pointer
                                            .interact_pos()
                                            .is_some_and(|p| img_rect.contains(p))
                                });
                                if holding_image {
                                    Self::paint_enlarged_image(ui, &image.name, img_rect);
                                    ctx.set_cursor_icon(egui::CursorIcon::ZoomIn);
                                }
                            }
                        }
                    }
                    // Correct the estimate for rows we actually laid out.
                    // This only ever touches VISIBLE rows, so an off-screen
                    // row whose float changed height would keep a stale
                    // value — that case is covered by `float_epoch`, which
                    // forces a full re-measure of every row (see
                    // update_row_height_cache). Reserved float space lives in
                    // `extra` and is deliberately NOT written here, or it
                    // would be erased every frame.
                    if (cache.heights[slot] - height).abs() > 0.5 {
                        cache.heights[slot] = height;
                    }

                    let char_at =
                        |pos: Pos2| galley.cursor_from_pos(pos - galley_pos).index.0;
                    let link_at = |pos: Pos2| {
                        let c = char_at(pos);
                        links
                            .iter()
                            .find(|(range, _)| range.contains(&c))
                            .map(|(_, link)| link)
                    };

                    let hovered_link = if response.hovered() {
                        pointer.and_then(|pos| link_at(pos).cloned())
                    } else {
                        None
                    };
                    if response.hovered() {
                        ctx.set_cursor_icon(if hovered_link.is_some() {
                            egui::CursorIcon::PointingHand
                        } else {
                            egui::CursorIcon::Text
                        });
                    }

                    // Press: anchor a new selection (or extend with Shift),
                    // unless this press starts a modifier item-drag on a link.
                    if response.is_pointer_button_down_on() && any_pressed {
                        press_claimed_by_line = true;
                        if let Some(pos) = press_pos {
                            let starts_item_drag = Self::link_drag_modifier_down(ui)
                                && link_at(pos).is_some_and(Self::link_is_draggable);
                            if !starts_item_drag && !is_touch {
                                let c = char_at(pos);
                                let extend = ui.input(|i| i.modifiers.shift);
                                match (&mut selection, extend) {
                                    (Some(sel), true) if sel.scroll_id == scroll_id => {
                                        sel.head = (uid, c);
                                        sel.dragging = true;
                                    }
                                    _ => {
                                        selection = Some(GuiBufferSelection {
                                            scroll_id: scroll_id.to_string(),
                                            anchor: (uid, c),
                                            head: (uid, c),
                                            dragging: true,
                                        });
                                    }
                                }
                                Self::store_buffer_selection(&ctx, selection.clone());
                            }
                        }
                    }

                    // Double-click selects the word, triple-click the line.
                    if response.double_clicked() {
                        if let Some(pos) = pointer {
                            let (word_start, word_end) =
                                Self::word_char_range(galley.text(), char_at(pos));
                            selection = Some(GuiBufferSelection {
                                scroll_id: scroll_id.to_string(),
                                anchor: (uid, word_start),
                                head: (uid, word_end),
                                dragging: false,
                            });
                            Self::store_buffer_selection(&ctx, selection.clone());
                        }
                    } else if response.triple_clicked() {
                        selection = Some(GuiBufferSelection {
                            scroll_id: scroll_id.to_string(),
                            anchor: (uid, 0),
                            head: (uid, galley.end().index.0),
                            dragging: false,
                        });
                        Self::store_buffer_selection(&ctx, selection.clone());
                    }

                    // Plain click on a link fires it.
                    if response.clicked() && clicked_link.is_none() {
                        let click_pos = response
                            .interact_pointer_pos()
                            .or(pointer)
                            .unwrap_or(Pos2::ZERO);
                        if let Some(link) = link_at(click_pos) {
                            clicked_link = Some(GuiLinkClick {
                                link_data: link.clone(),
                                click_pos: Self::click_pos_to_grid(click_pos),
                            });
                        }
                    }

                    // Modifier+drag on a draggable link starts an item drag;
                    // releasing one link onto another emits a drop action.
                    if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                        if response.is_pointer_button_down_on()
                            && Self::link_drag_modifier_down(ui)
                            && rect.contains(origin)
                        {
                            if let Some(link) =
                                link_at(origin).filter(|link| Self::link_is_draggable(link))
                            {
                                response.dnd_set_drag_payload(link.clone());
                            }
                        }
                    }
                    // Only consult (and thereby consume) the drag payload when
                    // the release lands on an actual link; a release on the
                    // blank part of a row must leave the payload for the
                    // window-level fallback that resolves body drops
                    // ("_drag #id drop" on the main window, hands, etc.).
                    if let Some(target) = pointer.and_then(link_at) {
                        if let Some(dragged) = response.dnd_release_payload::<LinkData>() {
                            if dragged.exist_id != target.exist_id && clicked_link.is_none() {
                                clicked_link = Some(GuiLinkClick {
                                    link_data: LinkData {
                                        exist_id: Self::LINK_DROP_SENTINEL.to_string(),
                                        noun: format!(
                                            "{}|{}",
                                            dragged.exist_id, target.exist_id
                                        ),
                                        text: String::new(),
                                        coord: None,
                                    },
                                    click_pos: (0, 0),
                                });
                            }
                        }
                    }

                    if ui.is_rect_visible(rect) {
                        if let Some(((line0, char0), (line1, char1))) = &paint_range {
                            if *line0 <= line_index && line_index <= *line1 {
                                let from = if line_index == *line0 { *char0 } else { 0 };
                                let to = if line_index == *line1 {
                                    *char1
                                } else {
                                    galley.end().index.0
                                };
                                let range = egui::text_selection::CCursorRange::two(
                                    egui::text::CCursor::new(from),
                                    egui::text::CCursor::new(to),
                                );
                                egui::text_selection::visuals::paint_text_selection(
                                    &mut galley,
                                    ui.visuals(),
                                    &range,
                                    None,
                                );
                            }
                        }
                        ui.painter()
                            .galley(galley_pos, galley.clone(), visuals.text_color());
                        super::color_emoji::paint_color_emoji(
                            &ctx,
                            ui.painter(),
                            &galley,
                            galley_pos,
                        );
                        // Custom emoji images over their `:name:` slots.
                        if !custom_runs.is_empty() {
                            Self::paint_custom_emoji_runs(
                                &ctx,
                                ui.painter(),
                                &galley,
                                galley_pos,
                                &custom_runs,
                            );
                        }
                    }
                }
                // A press on the blank area below the last line clears the
                // selection (presses on lines were handled above; presses
                // outside the viewport were handled before the loop).
                if any_pressed
                    && !press_claimed_by_line
                    && press_pos.is_some_and(|pos| clip.contains(pos))
                    && selection
                        .as_ref()
                        .is_some_and(|sel| sel.scroll_id == scroll_id)
                {
                    Self::store_buffer_selection(&ctx, None);
                }
                let bottom_space: f32 =
                    cache.stride_sum(last_visible..cache.heights.len(), spacing_y);
                if bottom_space > spacing_y {
                    ui.allocate_space(Vec2::new(1.0, bottom_space - spacing_y));
                }
            });
        // Next frame's anchoring pre-pass targets this area's real id.
        outer_ctx.data_mut(|data| data.insert_temp(area_id_key, output.id));

        // Resolve `follow_bottom` against the real layout.
        //
        // A user scroll that comes to rest within a row of the end counts as
        // "at the bottom" and resumes following — egui's own re-stick needed
        // exact float equality, which a scrollbar drag or kinetic flick
        // almost never lands on, leaving the window permanently unstuck. A
        // tolerance is the whole fix; no snapping or shadow state required.
        let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
        if follow_bottom && !user_scrolled {
            // Correct the cache-derived estimate against the real layout, so
            // a stale height (an image that just decoded, a font swap) can
            // never leave the tail slightly off-screen. Not on a user-input
            // frame: that would undo the gesture egui just applied.
            if (output.state.offset.y - max_offset).abs() > 0.5 {
                if let Some(mut state) = egui::scroll_area::State::load(&outer_ctx, output.id) {
                    state.offset.y = max_offset;
                    state.store(&outer_ctx, output.id);
                    outer_ctx.request_repaint();
                }
            }
        } else {
            let tolerance =
                outer_ctx.fonts_mut(|fonts| fonts.row_height(font_id)) + outer_spacing_y;
            let at_rest = !outer_ctx.input(|i| i.pointer.any_down());
            if at_rest && max_offset - output.state.offset.y <= tolerance {
                follow_bottom = true;
            }
        }
        outer_ctx.data_mut(|data| data.insert_temp(follow_key, follow_bottom));

        clicked_link
    }

}
