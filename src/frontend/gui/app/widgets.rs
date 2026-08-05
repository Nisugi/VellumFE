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
        Self::segment_text_format_ex(segment, visuals, search_match, false, font_id)
    }

    /// As segment_text_format, with the link fallback color when `is_link`.
    fn segment_text_format_ex(
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
    fn flush_text_job(
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
    fn paint_custom_emoji_runs(
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
    fn add_label_with_color_emoji(
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
    fn format_line_timestamp(
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
    fn line_tail_selection_filler(ui: &mut egui::Ui, font_id: &egui::FontId) {
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
    fn emoji_placeholder(ctx: &egui::Context, font_id: &egui::FontId) -> String {
        let row_h = ctx.fonts_mut(|f| f.row_height(font_id));
        let space_w = ctx.fonts_mut(|f| f.glyph_width(font_id, ' ')).max(1.0);
        let target_w = row_h * super::custom_emoji_render::width_factor();
        let n = (target_w / space_w).ceil().max(1.0) as usize;
        " ".repeat(n)
    }

    fn build_line_job(
        ctx: &egui::Context,
        line: &StyledLine,
        visuals: &egui::Visuals,
        search_query: Option<&str>,
        font_id: &egui::FontId,
        wrap_width: f32,
        timestamps: Option<crate::config::TimestampPosition>,
    ) -> GuiLineJob {
        let mut job = egui::text::LayoutJob {
            wrap: egui::text::TextWrapping {
                max_width: wrap_width,
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
    fn compose_line_text(
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

    fn buffer_selection_data_id() -> egui::Id {
        egui::Id::new("vellum_buffer_text_selection")
    }

    fn buffer_selection(ctx: &egui::Context) -> Option<GuiBufferSelection> {
        ctx.data(|data| data.get_temp(Self::buffer_selection_data_id()))
    }

    /// True when a game-window buffer selection spans a non-empty range, i.e.
    /// the user has text highlighted that should own Copy/Cut over the command
    /// input. A collapsed selection (anchor == head) or none returns false.
    fn active_buffer_selection_present(ctx: &egui::Context) -> bool {
        Self::buffer_selection(ctx).is_some_and(|sel| sel.anchor != sel.head)
    }

    fn store_buffer_selection(ctx: &egui::Context, selection: Option<GuiBufferSelection>) {
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
    fn resolve_line_uid(base_uid: u64, len: usize, uid: u64) -> usize {
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
    fn ordered_selection_endpoints(
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
    fn slice_line_by_chars(text: &str, from: Option<usize>, to: Option<usize>) -> &str {
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
    fn buffer_selection_copy_text(
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
    fn word_char_range(text: &str, at: usize) -> (usize, usize) {
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
                        column.add_sized([column.available_width().max(40.0), bar_height], bar);
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
                    ui.add_sized([ui.available_width().max(40.0), bar_height], bar);
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
    fn countdown_remaining_seconds_f(
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

    /// ProfanityFE injury palette: none, injury 1-3, scar 1-3.
    /// The default severity palette as egui colors, parsed once from the
    /// shared `DEFAULT_INJURY_PALETTE` (single source of truth with the TUI and
    /// web). Used when no per-widget config override applies (e.g. the
    /// other-player injuries popup).
    pub(super) fn default_injury_palette() -> [Color32; 7] {
        std::array::from_fn(|i| {
            parse_hex_color(crate::config::DEFAULT_INJURY_PALETTE[i])
                .unwrap_or(Color32::GRAY)
        })
    }

    /// Resolve an injury doll's palette from its config (per-level overrides
    /// over the shared defaults) into egui colors. The GUI previously ignored
    /// these config fields entirely, so a user's injury*_color/scar*_color
    /// changed the TUI but not the GUI.
    pub(super) fn resolved_injury_palette(
        data: &crate::config::InjuryDollWidgetData,
    ) -> [Color32; 7] {
        let hex = data.resolved_colors();
        std::array::from_fn(|i| {
            parse_hex_color(&hex[i]).unwrap_or_else(|| Self::default_injury_palette()[i])
        })
    }

    /// Look up the fill color for a severity level in a resolved palette.
    pub(super) fn injury_level_color(palette: &[Color32; 7], level: u8) -> Color32 {
        palette[level.min(6) as usize]
    }

    /// Human-readable body part name for a hover tooltip ("leftArm" ->
    /// "left arm"); unknown protocol keys pass through unchanged.
    fn doll_part_display_name(part: &str) -> &str {
        crate::config::skins::DOLL_PARTS
            .iter()
            .find(|(key, _, _)| key.eq_ignore_ascii_case(part))
            .map(|(_, display, _)| *display)
            .unwrap_or(part)
    }

    /// Human-readable severity for a hover tooltip.
    fn injury_severity_text(level: u8) -> &'static str {
        match level.min(6) {
            0 => "uninjured",
            1 => "minor injury",
            2 => "moderate injury",
            3 => "severe injury",
            4 => "minor scar",
            5 => "moderate scar",
            _ => "severe scar",
        }
    }

    /// Wrayth-style paperdoll drawn with painter geometry: each body part is
    /// a shape filled by its injury color, with a hover tooltip naming the
    /// part and severity. Back and nervous system have no spot on a front
    /// silhouette, so they render as "B"/"N" letters in the bottom corners
    /// (Wrayth-style). Scales with the window and needs no image assets.
    pub(super) fn render_injury_doll(
        ui: &mut egui::Ui,
        injuries: &HashMap<String, u8>,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
        grayscale: bool,
        palette: &[Color32; 7],
    ) {
        // Sprite mode: skin-supplied base body, then per part either a
        // hand-drawn severity overlay (authored on the base's canvas so it
        // stacks in place) or a generated dot at the part's calibrated
        // anchor point.
        if let Some(base) = skin_art.and_then(|art| art.doll_base) {
            let art = skin_art.unwrap();
            // Grayscale twins exist only while the checkbox demands them;
            // the generated dots keep their colors regardless.
            let base = if grayscale {
                art.doll_base_gray.unwrap_or(base)
            } else {
                base
            };
            let avail = ui.available_size();
            let (outer, response) = ui.allocate_exact_size(
                Vec2::new(avail.x.max(40.0), avail.y.max(60.0)),
                egui::Sense::hover(),
            );
            let painter = ui.painter().with_clip_rect(outer);
            let dest = crate::frontend::gui::skin::sprite_dest(&base, outer);
            crate::frontend::gui::skin::paint_sprite(&painter, dest, &base, Color32::WHITE);
            let dot_radius =
                (art.doll_dots.diameter * dest.height() / 2.0).max(4.0);
            let mut wounds: Vec<String> = Vec::new();
            for (part, level) in injuries {
                if *level == 0 {
                    continue;
                }
                let overlay = if grayscale {
                    art.doll_overlay_gray(part, *level)
                        .or_else(|| art.doll_overlay(part, *level))
                } else {
                    art.doll_overlay(part, *level)
                };
                if let Some(overlay) = overlay {
                    crate::frontend::gui::skin::paint_sprite(
                        &painter,
                        dest,
                        &overlay,
                        Color32::WHITE,
                    );
                } else {
                    let anchor = art.doll_anchor(part);
                    let center = dest.min
                        + Vec2::new(anchor.x * dest.width(), anchor.y * dest.height());
                    crate::frontend::gui::skin::paint_severity_dot(
                        &painter,
                        center,
                        dot_radius,
                        *level,
                        &art.doll_dots,
                    );
                }
                wounds.push(format!(
                    "{}: {}",
                    Self::doll_part_display_name(part),
                    Self::injury_severity_text(*level)
                ));
            }
            // No "uninjured" tooltip on an unwounded doll (it read as a stray
            // badge over the UberBar paperdoll); only surface actual wounds.
            if !wounds.is_empty() {
                wounds.sort();
                response.on_hover_text(wounds.join("\n"));
            }
            return;
        }

        // (key, display name, shape) in unit coordinates: x/y are fractions
        // of the doll rect; radii and line widths are fractions of its
        // height. Head must precede eyes so the eyes paint on top.
        enum PartShape {
            Circle { c: (f32, f32), r: f32 },
            Block { min: (f32, f32), max: (f32, f32) },
            Line { a: (f32, f32), b: (f32, f32), w: f32 },
            Letter { c: (f32, f32), letter: &'static str },
        }
        use PartShape::*;
        // Back and nervous system have no spot on a front silhouette; like
        // Wrayth's paperdoll they render as "B" and "N" letters in the
        // bottom corners, colored by severity.
        const PARTS: &[(&str, &str, PartShape)] = &[
            ("head", "head", Circle { c: (0.50, 0.105), r: 0.085 }),
            ("leftEye", "left eye", Circle { c: (0.465, 0.09), r: 0.018 }),
            ("rightEye", "right eye", Circle { c: (0.535, 0.09), r: 0.018 }),
            ("neck", "neck", Block { min: (0.465, 0.19), max: (0.535, 0.235) }),
            ("chest", "chest", Block { min: (0.38, 0.235), max: (0.62, 0.41) }),
            ("abdomen", "abdomen", Block { min: (0.395, 0.41), max: (0.605, 0.525) }),
            ("leftArm", "left arm", Line { a: (0.365, 0.26), b: (0.265, 0.47), w: 0.045 }),
            ("rightArm", "right arm", Line { a: (0.635, 0.26), b: (0.735, 0.47), w: 0.045 }),
            ("leftHand", "left hand", Circle { c: (0.25, 0.515), r: 0.033 }),
            ("rightHand", "right hand", Circle { c: (0.75, 0.515), r: 0.033 }),
            ("leftLeg", "left leg", Line { a: (0.44, 0.53), b: (0.41, 0.90), w: 0.055 }),
            ("rightLeg", "right leg", Line { a: (0.56, 0.53), b: (0.59, 0.90), w: 0.055 }),
            ("back", "back", Letter { c: (0.12, 0.93), letter: "B" }),
            ("nsys", "nervous system", Letter { c: (0.88, 0.93), letter: "N" }),
        ];

        // Fit an aspect-stable doll rect into the available space, centered
        // horizontally so narrow and wide windows both look intentional.
        const ASPECT: f32 = 0.75; // width : height
        let avail = ui.available_size();
        let mut height = avail.y.max(60.0);
        let mut width = height * ASPECT;
        if width > avail.x.max(40.0) {
            width = avail.x.max(40.0);
            height = width / ASPECT;
        }
        let (outer, _) =
            ui.allocate_exact_size(Vec2::new(avail.x.max(width), height), egui::Sense::hover());
        let rect = Rect::from_center_size(outer.center(), Vec2::new(width, height));
        let painter = ui.painter().with_clip_rect(outer);
        let at = |x: f32, y: f32| rect.min + Vec2::new(x * rect.width(), y * rect.height());
        let scale = rect.height();

        let letter_font = egui::FontId::proportional((scale * 0.09).clamp(10.0, 18.0));
        for (key, display, shape) in PARTS {
            let level = injuries.get(*key).copied().unwrap_or(0);
            let fill = Self::injury_level_color(palette, level);
            let outline = egui::Stroke::new(1.0, Self::lighten(fill, 0.2));

            let hover_rect = match shape {
                Circle { c, r } => {
                    let center = at(c.0, c.1);
                    painter.circle(center, r * scale, fill, outline);
                    Rect::from_center_size(center, Vec2::splat(r * scale * 2.0))
                }
                Block { min, max } => {
                    let shape_rect = Rect::from_min_max(at(min.0, min.1), at(max.0, max.1));
                    painter.rect(shape_rect, scale * 0.02, fill, outline, egui::StrokeKind::Middle);
                    shape_rect
                }
                Line { a, b, w } => {
                    let (a, b) = (at(a.0, a.1), at(b.0, b.1));
                    painter.line_segment([a, b], egui::Stroke::new(w * scale, fill));
                    Rect::from_two_pos(a, b).expand(w * scale * 0.5)
                }
                Letter { c, letter } => {
                    let center = at(c.0, c.1);
                    painter.text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        *letter,
                        letter_font.clone(),
                        fill,
                    );
                    Rect::from_center_size(center, Vec2::splat(scale * 0.11))
                }
            };

            ui.interact(
                hover_rect,
                ui.id().with(("injury_doll", key)),
                egui::Sense::hover(),
            )
            .on_hover_text(format!("{}: {}", display, Self::injury_severity_text(level)));
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
                ui.allocate_ui(Vec2::new(170.0, 225.0), |ui| {
                    // Another player's injuries: no per-widget config, so the
                    // shared default palette.
                    Self::render_injury_doll(
                        ui,
                        &popup.injuries,
                        self.skin_state.widget_art().as_deref(),
                        self.ui_settings.doll_grayscale,
                        &Self::default_injury_palette(),
                    );
                });
            });
        if !open {
            self.app_core.ui_state.injuries_popup = None;
        }
    }

    pub(super) fn render_indicator_content(
        ui: &mut egui::Ui,
        label: &str,
        indicator: &crate::data::IndicatorData,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
        gray_inactive: bool,
        resolved: &crate::core::conditions::ResolvedStatusArt,
    ) {
        let text = if label.is_empty() {
            &indicator.indicator_id
        } else {
            label
        };
        // A matched state's color wins; then the per-window color; then the
        // TUI defaults (#00ff00 active, #555555 off).
        let color = resolved
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| {
                if indicator.active {
                    indicator
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(Color32::from_rgb(0x00, 0xff, 0x00))
                } else {
                    Color32::from_rgb(0x55, 0x55, 0x55)
                }
            });
        // Icon precedence: a resolved IconRef (state icon or template active
        // icon) via the skin/pool, then — only when ACTIVE — the id-keyed skin
        // sprite and the built-in pictogram; custom ids without art keep the
        // text. When INACTIVE with no configured inactive icon (resolved.icon
        // is None), render NOTHING: inactive art is opt-in, never a dimmed
        // copy or a fallback pictogram. "Gray when inactive" still applies to
        // a configured inactive sprite.
        let inactive_blank = !indicator.active && resolved.icon.is_none();
        // Nothing to draw and no active pictogram to fall back to: leave the
        // cell blank (inactive with no configured inactive icon).
        if inactive_blank {
            return;
        }
        let mut grayed = false;
        let sprite = match &resolved.icon {
            Some(icon) => skin_art.and_then(|art| {
                if !indicator.active && gray_inactive {
                    // Grayscale twin of the configured inactive sprite, if any.
                    if let Some(gray) = art.icon_gray(&indicator.indicator_id) {
                        grayed = true;
                        return Some(gray);
                    }
                }
                art.resolve_icon_ref(icon, &indicator.indicator_id)
            }),
            // Active + no explicit icon: fall through to the id-keyed skin
            // sprite (and the built-in pictogram below) so "Default (by id)"
            // shows the built-in art.
            None => skin_art.and_then(|art| art.icon(&indicator.indicator_id)),
        };
        if sprite.is_some() || super::status_icons::supported(&indicator.indicator_id) {
            let side = ui
                .available_width()
                .min(ui.available_height())
                .clamp(10.0, 96.0);
            ui.centered_and_justified(|ui| {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
                if let Some(sprite) = sprite {
                    // Sprites carry their own colors: full-color when
                    // active, dimmed toward gray when inactive (or the
                    // full-strength gray twin when that setting is on).
                    let tint = if indicator.active || grayed {
                        Color32::WHITE
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 70)
                    };
                    let dest = crate::frontend::gui::skin::icon_dest(&sprite, rect);
                    crate::frontend::gui::skin::paint_icon(ui.painter(), dest, &sprite, tint);
                } else {
                    super::status_icons::paint(
                        ui.painter(),
                        rect,
                        &indicator.indicator_id,
                        color,
                        ui.visuals().window_fill(),
                    );
                }
                response.on_hover_text(text.to_string());
            });
            return;
        }
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(text).color(color).strong());
        });
    }

    /// Mini map: follows the current room (auto-centered, auto-switching
    /// between the outdoor sheet and the interior group the character is
    /// in); clicking a room walks there via `;go2`.
    pub(super) fn render_map_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        map_data: &crate::data::MapData,
        zoom_override: Option<f32>,
    ) -> Option<GuiLinkClick> {
        use crate::core::map_service::DbState;
        use crate::frontend::gui::map_view::{self, MapCamera, MapStyle};

        let map = &app_core.map;
        let hint = |ui: &mut egui::Ui, text: &str| {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(text).weak());
            });
        };
        match map.db_state() {
            DbState::NotLoaded => {
                hint(
                    ui,
                    "Download map data in Settings > Map (or point at your Lich folder)",
                );
                return None;
            }
            DbState::Loading => {
                ui.centered_and_justified(|ui| ui.spinner());
                return None;
            }
            DbState::Failed => {
                let msg = map
                    .db_error
                    .clone()
                    .unwrap_or_else(|| "mapdb load failed".to_string());
                hint(ui, &format!("Map unavailable: {msg}"));
                return None;
            }
            DbState::Loaded => {}
        }
        let Some(scene) = map.current_scene() else {
            let msg = if map.current_location.is_some() {
                "Generating map..."
            } else {
                "Waiting for a mapped room..."
            };
            hint(ui, msg);
            return None;
        };

        let current = map.current_room_id;
        let (sheet_kind, center, group_filter) = match current.and_then(|id| scene.room(id)) {
            // Indoors, show just the building the character is in (its whole
            // cluster of connected interior groups) — the full interiors
            // shelf is explorer territory.
            Some((sheet, room)) => (
                sheet,
                room.cell,
                (sheet == crate::core::layout_engine::Sheet::Interiors)
                    .then(|| scene.cluster_groups(room.group)),
            ),
            None => {
                let b = &scene.outdoor;
                (
                    crate::core::layout_engine::Sheet::Outdoor,
                    crate::core::layout_engine::Cell {
                        x: (b.min.x + b.max.x) / 2,
                        y: (b.min.y + b.max.y) / 2,
                    },
                    None,
                )
            }
        };

        // Unmapped interiors: session ghost sketches hang off their anchor
        // room; standing in one moves the camera and the current ring to it.
        // Rendered only in cartography mode — everyday play shows mapdb truth.
        let ghost_overlay = (app_core.config.map.mapping_mode && !map.ghosts().is_empty()).then(|| {
            crate::core::ghost_rooms::build_overlay(
                map.ghosts(),
                scene,
                sheet_kind,
                group_filter.as_ref(),
            )
        });
        let current_ghost_cell = map
            .current_ghost
            .and_then(|uid| ghost_overlay.as_ref()?.cell_of(uid));
        let center = current_ghost_cell.unwrap_or(center);

        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        if rect.width() < 8.0 || rect.height() < 8.0 {
            return None;
        }
        // Glide toward the new room instead of jump-cutting.
        let cx = ui.ctx().animate_value_with_time(
            ui.id().with("map_center_x"),
            center.x as f32,
            0.25,
        );
        let cy = ui.ctx().animate_value_with_time(
            ui.id().with("map_center_y"),
            center.y as f32,
            0.25,
        );
        let camera = map_view::MapCamera {
            center: egui::Pos2::new(cx, cy),
            px_per_cell: zoom_override.unwrap_or(map_data.zoom).clamp(2.0, 96.0),
        };
        let style = MapStyle::from_visuals(ui.visuals());
        let compass = Some(app_core.game_state.compass_dirs.as_slice());
        // While standing in a ghost, the ring and compass ticks belong to the
        // sketch, not the held anchor room.
        let in_ghost = current_ghost_cell.is_some();
        let result = map_view::paint_sheet(
            ui,
            rect,
            scene.sheet(sheet_kind),
            camera,
            if in_ghost { None } else { current },
            if in_ghost { None } else { compass },
            true,
            group_filter.as_ref(),
            &style,
        );
        if let Some(overlay) = ghost_overlay.as_ref().filter(|o| !o.is_empty()) {
            map_view::paint_ghosts(
                ui,
                rect,
                overlay,
                camera,
                map.current_ghost,
                if in_ghost { compass } else { None },
                &style,
            );
        }

        // Travel progress rides on the map while the walk executor runs.
        if let (Some(task), Some(current)) = (app_core.travel.task(), map.current_room_id) {
            if let Some(db) = map.mapdb() {
                let done = task.rooms_total().saturating_sub(task.rooms_remaining());
                let label = format!(
                    "-> {} | {}/{} rooms | ETA {}",
                    task.destination,
                    done,
                    task.rooms_total(),
                    crate::core::travel::format_eta(task.eta_seconds(db, current))
                );
                let font = egui::FontId::proportional(12.0);
                let galley = ui.painter().layout_no_wrap(
                    label,
                    font.clone(),
                    ui.visuals().strong_text_color(),
                );
                let pad = egui::vec2(6.0, 3.0);
                let banner = egui::Rect::from_min_size(
                    rect.left_top() + egui::vec2(4.0, 4.0),
                    galley.size() + pad * 2.0,
                );
                let painter = ui.painter().with_clip_rect(rect);
                painter.rect_filled(
                    banner,
                    3.0,
                    ui.visuals().extreme_bg_color.gamma_multiply(0.85),
                );
                painter.galley(banner.min + pad, galley, ui.visuals().strong_text_color());
            }
        }

        result.clicked_room.map(|id| GuiLinkClick {
            link_data: Self::direct_command_link(if app_core.config.go2.native_map_clicks {
                format!(".go2 {id}")
            } else {
                format!(";go2 {id}")
            }),
            click_pos: Self::click_pos_to_grid(
                ui.ctx().pointer_latest_pos().unwrap_or(Pos2::ZERO),
            ),
        })
    }

    pub(super) fn render_compass_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        compass_data: &crate::data::CompassData,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
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

        ui.horizontal_centered(|ui| {
            // Rose square: whatever height we have, leaving room for the
            // up/down arrow column to the right. Out is the rose's hub.
            let arrow_side = (ui.available_height() * 0.28).clamp(14.0, 30.0);
            let side = ui
                .available_height()
                .min(ui.available_width() - arrow_side - 8.0)
                .max(40.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
            if let Some(click) = Self::paint_compass_rose(ui, rect, &available, skin_art) {
                clicked_link = Some(click);
            }

            ui.vertical(|ui| {
                for (direction, points_up) in [("up", true), ("down", false)] {
                    if let Some(click) =
                        Self::paint_vertical_arrow(ui, arrow_side, direction, points_up, &available)
                    {
                        if clicked_link.is_none() {
                            clicked_link = Some(click);
                        }
                    }
                }
            });
        });

        clicked_link
    }

    /// One up/down movement arrow beside the compass rose: a triangle in
    /// the same color language as the rose (link color when the exit is
    /// available, faint outline otherwise), clickable like a rose arrow.
    fn paint_vertical_arrow(
        ui: &mut egui::Ui,
        side: f32,
        direction: &str,
        points_up: bool,
        available: &HashSet<String>,
    ) -> Option<GuiLinkClick> {
        let is_available = available.contains(direction);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(side),
            if is_available {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );

        let visuals = ui.visuals();
        let available_fill = visuals.hyperlink_color;
        let idle_stroke = visuals.widgets.noninteractive.bg_stroke.color;
        let (fill, stroke) = if !is_available {
            (Color32::TRANSPARENT, egui::Stroke::new(1.0, idle_stroke))
        } else if response.hovered() {
            let hover = Self::lighten(available_fill, 0.35);
            (hover, egui::Stroke::new(1.0, hover))
        } else {
            (available_fill, egui::Stroke::new(1.0, available_fill))
        };

        let inner = rect.shrink(side * 0.18);
        let points = if points_up {
            vec![
                Pos2::new(inner.center().x, inner.min.y),
                Pos2::new(inner.min.x, inner.max.y),
                Pos2::new(inner.max.x, inner.max.y),
            ]
        } else {
            vec![
                Pos2::new(inner.min.x, inner.min.y),
                Pos2::new(inner.max.x, inner.min.y),
                Pos2::new(inner.center().x, inner.max.y),
            ]
        };
        ui.painter()
            .add(egui::Shape::convex_polygon(points, fill, stroke));

        if is_available {
            let response = response
                .on_hover_text(direction.to_string())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                return Some(Self::gui_link_click_from_response(
                    &response,
                    ui,
                    Self::direct_command_link(direction.to_string()),
                ));
            }
        }
        None
    }

    /// Draw the compass rose into `rect`. Sprite mode (skin `[compass]`
    /// with a rose image) paints the rose plus a lit overlay per available
    /// direction, all aspect-fit to the same canvas. Vector mode draws
    /// eight arrows around a hub, available exits filled with the theme
    /// link color, the rest as faint outlines. Both modes share the same
    /// clickable hit regions and send the same movement commands.
    fn paint_compass_rose(
        ui: &mut egui::Ui,
        rect: Rect,
        available: &HashSet<String>,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> Option<GuiLinkClick> {
        const DIRECTIONS: [(&str, &str); 8] = [
            ("n", "north"),
            ("ne", "northeast"),
            ("e", "east"),
            ("se", "southeast"),
            ("s", "south"),
            ("sw", "southwest"),
            ("w", "west"),
            ("nw", "northwest"),
        ];

        let mut clicked_link = None;
        let painter = ui.painter().with_clip_rect(rect);
        let center = rect.center();
        let radius = rect.width().min(rect.height()) * 0.5 - 2.0;
        if radius < 8.0 {
            return None;
        }

        let rose_sprite = skin_art.and_then(|art| art.compass_rose);
        if let Some(rose) = &rose_sprite {
            let dest = crate::frontend::gui::skin::sprite_dest(rose, rect);
            crate::frontend::gui::skin::paint_sprite(&painter, dest, rose, Color32::WHITE);
            let overlay_dirs = DIRECTIONS
                .iter()
                .map(|(direction, _)| *direction)
                .chain(["up", "down", "out"]);
            for direction in overlay_dirs {
                if !available.contains(direction) {
                    continue;
                }
                if let Some(overlay) = skin_art.and_then(|art| art.compass_dir(direction)) {
                    crate::frontend::gui::skin::paint_sprite(
                        &painter,
                        dest,
                        &overlay,
                        Color32::WHITE,
                    );
                }
            }
        }

        let visuals = ui.visuals().clone();
        let available_fill = visuals.hyperlink_color;
        let hover_fill = Self::lighten(available_fill, 0.35);
        let idle_stroke = visuals.widgets.noninteractive.bg_stroke.color;

        for (index, (direction, full_name)) in DIRECTIONS.iter().enumerate() {
            let is_cardinal = index % 2 == 0;
            let angle = index as f32 * std::f32::consts::FRAC_PI_4;
            let dir = Vec2::new(angle.sin(), -angle.cos());
            let perp = Vec2::new(-dir.y, dir.x);

            let tip_r = if is_cardinal { radius } else { radius * 0.78 };
            let base_r = radius * 0.3;
            let half_w = if is_cardinal { radius * 0.15 } else { radius * 0.11 };

            let is_available = available.contains(*direction);
            let hit_center = center + dir * ((tip_r + base_r) * 0.5);
            let hit_rect =
                Rect::from_center_size(hit_center, Vec2::splat((radius * 0.4).max(12.0)));
            let response = ui.interact(
                hit_rect,
                ui.id().with(("compass_rose", direction)),
                if is_available {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );

            if rose_sprite.is_none() {
                let points = vec![
                    center + dir * tip_r,
                    center + dir * base_r + perp * half_w,
                    center + dir * base_r - perp * half_w,
                ];
                let (fill, stroke) = if !is_available {
                    (
                        Color32::TRANSPARENT,
                        egui::Stroke::new(1.0, idle_stroke),
                    )
                } else if response.hovered() {
                    (hover_fill, egui::Stroke::new(1.0, hover_fill))
                } else {
                    (available_fill, egui::Stroke::new(1.0, available_fill))
                };
                painter.add(egui::Shape::convex_polygon(points, fill, stroke));
            }

            if is_available {
                let response = response
                    .on_hover_text(*full_name)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() && clicked_link.is_none() {
                    clicked_link = Some(Self::gui_link_click_from_response(
                        &response,
                        ui,
                        Self::direct_command_link(direction.to_string()),
                    ));
                }
            }
        }

        // Hub over the arrow bases doubles as the OUT exit: lit and
        // clickable when the room has one, a plain hub otherwise.
        let out_available = available.contains("out");
        let hub_radius = radius * 0.18;
        let hub_response = ui.interact(
            Rect::from_center_size(center, Vec2::splat(hub_radius * 2.0)),
            ui.id().with(("compass_rose", "out")),
            if out_available {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        if rose_sprite.is_none() {
            let hub_fill = if !out_available {
                visuals.window_fill()
            } else if hub_response.hovered() {
                hover_fill
            } else {
                available_fill
            };
            painter.circle(center, hub_radius, hub_fill, egui::Stroke::new(1.0, idle_stroke));
        }
        if out_available {
            let hub_response = hub_response
                .on_hover_text("out")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if hub_response.clicked() && clicked_link.is_none() {
                clicked_link = Some(Self::gui_link_click_from_response(
                    &hub_response,
                    ui,
                    Self::direct_command_link("out".to_string()),
                ));
            }
        }

        clicked_link
    }

    /// Blend a color toward white by `t` (0..=1), preserving alpha.
    fn lighten(color: Color32, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let channel = |c: u8| c.saturating_add(((255 - c) as f32 * t) as u8);
        Color32::from_rgba_unmultiplied(
            channel(color.r()),
            channel(color.g()),
            channel(color.b()),
            color.a(),
        )
    }

    pub(super) fn render_hand_content(
        ui: &mut egui::Ui,
        hand_prefix: &str,
        item: &Option<String>,
        link: &Option<LinkData>,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
        resolved: &crate::core::conditions::ResolvedHand,
        icon_size: f32,
    ) -> Option<GuiLinkClick> {
        let empty_text = if hand_prefix == "S" { "None" } else { "Empty" };
        let item_text = item
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or(empty_text);
        // A matched icon state's text wins over the bracket fallback.
        let icon_text = resolved.text.as_deref().unwrap_or(match hand_prefix {
            "L" => "[L]",
            "R" => "[R]",
            "S" => "[S]",
            _ => "[?]",
        });
        // Skin sprite for this hand (icons table: lefthand/righthand/spellhand);
        // a matched icon state overrides it (IconRef::None = force artless);
        // without either the bracket text stays.
        let icon_id = match hand_prefix {
            "L" => "lefthand",
            "R" => "righthand",
            _ => "spellhand",
        };
        let icon_sprite = match &resolved.icon {
            Some(icon) => skin_art.and_then(|art| art.resolve_icon_ref(icon, icon_id)),
            None => skin_art.and_then(|art| art.icon(icon_id)),
        };
        let icon_tint = resolved
            .icon_color
            .as_deref()
            .and_then(crate::frontend::gui::skin::parse_hex_rgb)
            .unwrap_or(Color32::WHITE);
        // Keep hand rows compact and content-sized so they don't request full window width.
        let display_text = if item_text.chars().count() > 56 {
            let mut truncated: String = item_text.chars().take(53).collect();
            truncated.push_str("...");
            truncated
        } else {
            item_text.to_string()
        };
        // The icon fills the window's height, so a taller hand window means a
        // bigger icon (drag to 2/4 "lines" for big art) and a short one a small
        // icon. The configured hand_icon_size is the floor so a freshly-placed
        // hand isn't tiny; available height (capped) sets the ceiling.
        let floor = icon_size.clamp(16.0, 48.0);
        let avail = ui.available_height().max(1.0);
        let icon_size = avail.clamp(floor.min(avail), 512.0);
        let row_height = ui.spacing().interact_size.y.max(16.0).max(icon_size);
        let icon_width = icon_size;
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
            if let Some(sprite) = icon_sprite {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(icon_width, row_height),
                    egui::Sense::hover(),
                );
                let dest = crate::frontend::gui::skin::icon_dest(&sprite, rect);
                crate::frontend::gui::skin::paint_icon(
                    ui.painter(),
                    dest,
                    &sprite,
                    icon_tint,
                );
            } else {
                let mut icon_rich = RichText::new(icon_text).monospace().strong();
                if let Some(color) = resolved
                    .icon_color
                    .as_deref()
                    .and_then(crate::frontend::gui::skin::parse_hex_rgb)
                {
                    icon_rich = icon_rich.color(color);
                }
                ui.add_sized(
                    [icon_width, row_height],
                    egui::Label::new(icon_rich),
                );
            }
            ui.add_space(icon_gap);
            let text_width = (ui.available_width() - handle_gutter_width).max(1.0);
            if let Some(link_data) = item_link {
                let response = ui
                    .add_sized(
                        [text_width, row_height],
                        egui::Label::new(
                            RichText::new(display_text).color(ui.visuals().hyperlink_color),
                        )
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

    /// Per-window field toggles for the gs4_experience widget, from its
    /// layout def: (level, mind bar, exp bar, total exp, ascension exp).
    /// Missing def falls back to the widget's classic three-line look.
    pub(super) fn gs4_experience_flags(
        app_core: &AppCore,
        window_name: &str,
    ) -> (bool, bool, bool, bool, bool) {
        match app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window_name)
        {
            Some(crate::config::WindowDef::GS4Experience { data, .. }) => (
                data.show_level,
                data.show_mind_bar,
                data.show_exp_bar,
                data.show_total_exp,
                data.show_ascension_exp,
            ),
            _ => (true, true, true, false, false),
        }
    }

    /// Per-window field toggles for the encum widget: (bar, blurb text).
    pub(super) fn encumbrance_flags(app_core: &AppCore, window_name: &str) -> (bool, bool) {
        match app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window_name)
        {
            Some(crate::config::WindowDef::Encumbrance { data, .. }) => {
                (data.show_bar, data.show_label)
            }
            _ => (true, true),
        }
    }

    /// Group digits in threes: 1234567 -> "1,234,567".
    fn format_thousands(value: u64) -> String {
        let digits = value.to_string();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, ch) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(ch);
        }
        out
    }

    pub(super) fn render_gs4_experience_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        window_name: &str,
        settings: &WidgetRenderSettings,
    ) {
        let exp = &app_core.game_state.gs4_experience;
        if exp.level_text.is_empty()
            && exp.mind_state_text.is_empty()
            && exp.next_level_text.is_empty()
            && exp.exp.is_none()
            && exp.ascension_exp.is_none()
        {
            ui.weak("No experience data yet.");
            return;
        }

        let (show_level, show_mind_bar, show_exp_bar, show_total_exp, show_ascension_exp) =
            Self::gs4_experience_flags(app_core, window_name);
        if show_level && !exp.level_text.is_empty() {
            ui.label(RichText::new(&exp.level_text).strong());
        }
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        if show_mind_bar && !exp.mind_state_text.is_empty() {
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
        if show_exp_bar && !exp.next_level_text.is_empty() {
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
        if show_total_exp {
            if let Some(total) = exp.exp {
                ui.label(format!("Exp: {}", Self::format_thousands(total)));
            }
        }
        if show_ascension_exp {
            if let Some(ascension) = exp.ascension_exp {
                ui.label(format!("Ascension: {}", Self::format_thousands(ascension)));
            }
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
        window_name: &str,
        settings: &WidgetRenderSettings,
    ) {
        let enc = &app_core.game_state.encumbrance;
        let (show_bar, show_label) = Self::encumbrance_flags(app_core, window_name);
        if show_bar {
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
        }
        if show_label && !enc.blurb.is_empty() {
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

    /// Render a resident dialog panel (combat, befriend, ...) from the
    /// accumulated dialog store using the game's anchor-grid layout.
    /// Buttons/links send their command; dropdowns send their selection
    /// command (the game echoes back new state); the spinbox edits in
    /// place and its value feeds `%id%` in sibling commands. Commands are
    /// queued on ui_state.pending_panel_commands (immutable AppCore here).
    pub(super) fn render_dialog_panel_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        dialog_id: &str,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) {
        let Some(dialog) = app_core.ui_state.dialog_store.get(dialog_id) else {
            ui.weak("Waiting for the game to send this panel…");
            return;
        };
        let queue = |cmd: String| {
            if !cmd.trim().is_empty() {
                app_core.ui_state.pending_panel_commands.borrow_mut().push(cmd);
            }
        };

        let positioned = dialog.positioned_controls();
        let (content_w, content_h) = positioned
            .as_ref()
            .map(|(_, size)| *size)
            .unwrap_or((190.0, 24.0));
        let (canvas_rect, _) =
            ui.allocate_exact_size(egui::vec2(content_w, content_h), egui::Sense::hover());
        let origin = canvas_rect.min;

        if let Some((controls, _)) = &positioned {
            use crate::data::ui_state::PositionedControlKind;
            for control in controls {
                let (x, y, w, h) = control.rect;
                let rect =
                    egui::Rect::from_min_size(origin + egui::vec2(x, y), egui::vec2(w, h));
                match control.kind {
                    PositionedControlKind::Button(i) => {
                        if let Some(b) = dialog.buttons.get(i) {
                            let resp = ui.put(rect, egui::Button::new(&b.label).small());
                            let resp = match &b.command {
                                c if c.trim().is_empty() => resp,
                                _ => resp,
                            };
                            if resp.clicked() {
                                queue(dialog.command_with_placeholders(&b.command));
                            }
                        }
                    }
                    PositionedControlKind::DropDown(i) => {
                        if let Some(d) = dialog.dropdowns.get(i) {
                            if let Some(value) =
                                Self::dialog_panel_combo(ui, rect, dialog_id, d)
                            {
                                // Send the dropdown's command with the NEW
                                // value substituted (game echoes back state).
                                let mut probe = dialog.clone();
                                if let Some(slot) =
                                    probe.dropdowns.iter_mut().find(|x| x.id == d.id)
                                {
                                    slot.value = value;
                                }
                                queue(probe.command_with_placeholders(&d.command));
                            }
                        }
                    }
                    PositionedControlKind::ProgressBar(i) => {
                        if let Some(bar) = dialog.progress_bars.get(i) {
                            Self::paint_panel_progress_bar(ui, rect, bar);
                        }
                    }
                    PositionedControlKind::Label(i) => {
                        if let Some(label) = dialog.display_labels.get(i) {
                            Self::paint_panel_label(ui, rect, label);
                        }
                    }
                    PositionedControlKind::Skin(i) => {
                        // Backdrop art (skins are first in the list, so they
                        // paint behind the controls anchored to them).
                        if let Some(skin) = dialog.skins.get(i) {
                            Self::paint_dialog_skin(ui, rect, skin, dialog, skin_art);
                        }
                    }
                    // Anchor-only images (ubbars, wound points) are never drawn.
                    PositionedControlKind::Image(_) => {}
                }
            }
        }

        // Images an InjuriesPanel skin already draws as wound overlays on the
        // doll are display-only state — never surface them as a button row.
        // (A read-only reporter like UberBar copies Wrayth's cmd='cure ...' onto
        // its wound images, but it takes no input; the doll consumes them.)
        let doll_owned: std::collections::HashSet<&str> = dialog
            .skins
            .iter()
            .filter(|s| s.name.eq_ignore_ascii_case("InjuriesPanel"))
            .flat_map(|s| s.controls.iter().map(|c| c.as_str()))
            .collect();

        // Links and remaining images: combat's icon/link footer. Images with a
        // command render as buttons; the doll's wound images are excluded above.
        let footer_images: Vec<_> = dialog
            .images
            .iter()
            .filter(|image| !image.command.trim().is_empty())
            .filter(|image| !doll_owned.contains(image.id.as_str()))
            .collect();
        if !footer_images.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for image in footer_images {
                    let label = image.tooltip.as_deref().unwrap_or(&image.name);
                    if ui.small_button(label).clicked() {
                        queue(dialog.command_with_placeholders(&image.command));
                    }
                }
            });
        }
        if !dialog.links.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for link in &dialog.links {
                    if ui.link(&link.label).clicked() {
                        queue(dialog.command_with_placeholders(&link.command));
                    }
                }
            });
        }
    }

    /// Paint a dialog progress bar to its EXACT resolved rect (Wrayth pixel
    /// layout), instead of ui.put'ing an egui ProgressBar that centers itself
    /// at its own min-size and overflows the 15px rows UberBar uses. Trough +
    /// fill (fraction of width) + centered customText.
    fn paint_panel_progress_bar(
        ui: &egui::Ui,
        rect: egui::Rect,
        bar: &crate::data::DialogProgressBar,
    ) {
        let painter = ui.painter();
        let visuals = ui.visuals();
        let radius = 2.0;
        // Trough.
        painter.rect_filled(rect, radius, visuals.extreme_bg_color);
        // Fill.
        let frac = (bar.value.min(100) as f32 / 100.0).clamp(0.0, 1.0);
        if frac > f32::EPSILON {
            let mut fill_rect = rect;
            fill_rect.set_width(rect.width() * frac);
            painter.rect_filled(fill_rect, radius, visuals.selection.bg_fill);
        }
        // Centered text (auto-contrast against the ground it sits on).
        if !bar.text.is_empty() {
            let behind = if frac >= 0.5 {
                visuals.selection.bg_fill
            } else {
                visuals.extreme_bg_color
            };
            let color = Self::readable_text_color(visuals.text_color(), behind, true);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &bar.text,
                egui::FontId::proportional((rect.height() - 3.0).clamp(8.0, 14.0)),
                color,
            );
        }
    }

    /// Paint a dialog label to its EXACT rect honoring Wrayth `justify`
    /// (4 = left, 5 = center, 6 = right). UberBar right-justifies its value
    /// columns; ui.put centered them mid-slot, which read as floating gaps.
    fn paint_panel_label(
        ui: &egui::Ui,
        rect: egui::Rect,
        label: &crate::data::DialogLabel,
    ) {
        if label.value.is_empty() {
            return;
        }
        let (anchor, pos) = match label.justify {
            Some(6) => (egui::Align2::RIGHT_CENTER, rect.right_center()),
            Some(5) => (egui::Align2::CENTER_CENTER, rect.center()),
            // 4 or unspecified: left.
            _ => (egui::Align2::LEFT_CENTER, rect.left_center()),
        };
        ui.painter().text(
            pos,
            anchor,
            &label.value,
            egui::FontId::proportional((rect.height() - 3.0).clamp(8.0, 14.0)),
            ui.visuals().text_color(),
        );
    }

    /// Paint one `<skin>` backdrop inside `rect`. Wrayth scripts reference
    /// skin assets by the *client's* built-in names; the only one that maps to
    /// distinct art in VellumFE is `InjuriesPanel`, which we render as our own
    /// injury doll (base + shipped/calibrated anchors), with wound levels taken
    /// from the panel's own `<image>` data (`name='Injury3'` etc.) so wounds
    /// land on the right body regions.
    ///
    /// Bar skins (`healthBar`/`manaBar`/...) are intentionally ignored: they
    /// exist only to color a bar in Wrayth, and VellumFE already draws the
    /// sibling `<progressBar>` as its own filled, colored bar. Any other skin
    /// name paints nothing — the numeric bars and labels still show through.
    fn paint_dialog_skin(
        ui: &mut egui::Ui,
        rect: egui::Rect,
        skin: &crate::data::DialogSkin,
        dialog: &crate::data::ui_state::DialogState,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) {
        if !skin.name.eq_ignore_ascii_case("InjuriesPanel") {
            return;
        }
        // Build part -> severity from the panel's wound images, using the same
        // Injury1-3 / Scar1-6 convention as the game's injury feed.
        let mut injuries: HashMap<String, u8> = HashMap::new();
        for image in &dialog.images {
            let level = match image.name.as_str() {
                "Injury1" => 1,
                "Injury2" => 2,
                "Injury3" => 3,
                "Scar1" => 4,
                "Scar2" => 5,
                "Scar3" => 6,
                _ => 0,
            };
            if level > 0 {
                injuries.insert(image.id.clone(), level);
            }
        }
        // Confine the doll (which allocates its own space) to the skin's
        // resolved rect so it sits where the script positioned it.
        let builder = egui::UiBuilder::new().max_rect(rect);
        ui.scope_builder(builder, |ui| {
            Self::render_injury_doll(
                ui,
                &injuries,
                skin_art,
                false,
                &Self::default_injury_palette(),
            );
        });
    }

    /// A ComboBox for a dialog-panel dropdown; returns the newly picked
    /// value. Mirrors the popup dialog's dropdown_combo.
    fn dialog_panel_combo(
        ui: &mut egui::Ui,
        rect: egui::Rect,
        dialog_id: &str,
        dropdown: &crate::data::DialogDropDown,
    ) -> Option<String> {
        let selected_text = dropdown
            .options
            .iter()
            .find(|(_, value)| *value == dropdown.value)
            .map(|(text, _)| text.clone())
            .unwrap_or_else(|| dropdown.value.clone());
        let mut picked = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            egui::ComboBox::from_id_salt(("dialog_panel", dialog_id, &dropdown.id))
                .width(rect.width())
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (text, value) in &dropdown.options {
                        if ui
                            .selectable_label(*value == dropdown.value, text)
                            .clicked()
                            && *value != dropdown.value
                        {
                            picked = Some(value.clone());
                        }
                    }
                });
        });
        picked
    }

    pub(super) fn render_container_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        container_title: &str,
        wrap: bool,
    ) -> Option<GuiLinkClick> {
        let Some(container) = app_core.game_state.objects.find_container(container_title)
        else {
            ui.weak(format!("No contents cached for \"{}\".", container_title));
            return None;
        };

        let container_id = container.id.clone();
        let items: Vec<crate::core::game_objects::GameItem> = container.items.clone();

        let mut clicked_link: Option<GuiLinkClick> = None;
        let max_height = ui.available_height().max(1.0);
        let scroll_area = if wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };
        scroll_area
            .id_salt(format!("container_scroll_{}", container_id))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                if !wrap {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                }
                if items.is_empty() {
                    ui.weak("Empty.");
                    return;
                }
                // Registry items are structured; render each as a clickable,
                // draggable link (mirrors render_items_content). Dropping an
                // item onto the WINDOW BODY is handled by the window-level
                // drag-drop path (handle_link_drag_drop); here per-item drops
                // let you drag one item directly onto another.
                for item in &items {
                    let link = LinkData {
                        exist_id: item.id.clone(),
                        noun: item.noun.clone(),
                        text: item.name.clone(),
                        coord: None,
                    };
                    let response = ui
                        .add(
                            egui::Label::new(item.name.as_str())
                                .sense(egui::Sense::click_and_drag())
                                .selectable(!Self::link_drag_blocks_selection(ui)),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if let Some(drop) = Self::handle_link_dnd(ui, &response, &link) {
                        clicked_link.get_or_insert(drop);
                    }
                    if response.clicked() && clicked_link.is_none() {
                        clicked_link =
                            Some(Self::gui_link_click_from_response(&response, ui, link));
                    }
                }
            });
        clicked_link
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

    pub(super) fn render_hotkeybar_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        window_name: &str,
        bar_name: &str,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> Option<GuiLinkClick> {
        let Some(bar_def) = app_core.config.hotbars.find_bar(bar_name) else {
            ui.weak(format!(
                "Hotbar '{}' is not defined - use .hotbars to create it.",
                bar_name
            ));
            return None;
        };

        let now_server =
            chrono::Utc::now().timestamp() + app_core.message_processor.server_time_offset;
        let buttons = crate::core::hotbar::resolve_bar(
            bar_def,
            &app_core.game_state,
            now_server,
            app_core.gameobj_data_cached(),
        );

        // Countdown overlays tick between game events
        if buttons.iter().any(|b| b.countdown_secs.is_some()) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
        }

        let vertical = app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window_name)
            .is_some_and(|def| {
                matches!(
                    def,
                    crate::config::WindowDef::Hotkeybar { data, .. }
                        if data.orientation == "vertical"
                )
            });

        let mut clicked = None;
        let mut render_buttons = |ui: &mut egui::Ui| {
            for button in &buttons {
                use crate::config::IconMode;

                // Icon face: only when the mode asks for one AND the active
                // skin resolves the sheet cell. Otherwise fall back to text
                // (also the no-skin and TUI-authored-config behavior).
                let sprite = match button.icon_mode {
                    IconMode::Text => None,
                    IconMode::Icon | IconMode::IconAndLabel => {
                        button.icon.as_ref().and_then(|icon| {
                            skin_art.and_then(|art| {
                                // Dim states reuse the grayscale twin, barbar-style.
                                art.icon_ref_texture(
                                    &icon.icon,
                                    icon.grayscale || button.dim,
                                )
                            })
                        })
                    }
                };

                let mut response = if let Some((texture, uv)) = sprite {
                    let edge = Self::icon_edge(ui, bar_def.icon_size);
                    Self::draw_icon_button(ui, button, texture, uv, edge)
                } else {
                    let text = match button.countdown_secs {
                        Some(secs) if secs > 0 => {
                            format!("{}  {}s", button.label, secs)
                        }
                        _ => button.label.clone(),
                    };
                    let mut rich = RichText::new(text);
                    if button.dim {
                        rich = rich.color(ui.visuals().weak_text_color());
                    } else if let Some(fg) =
                        button.fg.as_deref().and_then(parse_hex_color)
                    {
                        rich = rich.color(fg);
                    }

                    let mut widget = egui::Button::new(rich);
                    if !button.dim {
                        if let Some(bg) = button.bg.as_deref().and_then(parse_hex_color)
                        {
                            widget = widget.fill(bg);
                        }
                    }
                    ui.add(widget)
                };

                let mut hover = button.tooltip.clone().unwrap_or_default();
                // Icon-only faces lose their text; surface the label on hover.
                if matches!(button.icon_mode, IconMode::Icon)
                    && sprite.is_some()
                    && !button.label.is_empty()
                {
                    hover = if hover.is_empty() {
                        button.label.clone()
                    } else {
                        format!("{}\n{}", button.label, hover)
                    };
                }
                if let Some(hotkey) = &button.hotkey {
                    if !hover.is_empty() {
                        hover.push('\n');
                    }
                    hover.push_str(&format!("[{}]", hotkey));
                }
                if !hover.is_empty() {
                    response = response.on_hover_text(hover);
                }

                if response.clicked() && clicked.is_none() {
                    clicked = Some(Self::gui_link_click_from_response(
                        &response,
                        ui,
                        Self::direct_command_link(button.command.clone()),
                    ));
                }
            }
        };

        if vertical {
            ui.vertical(render_buttons);
        } else {
            ui.horizontal_wrapped(render_buttons);
        }
        clicked
    }

    /// Icon face edge for a bar: its configured size (clamped sane) or the
    /// text-button height so mixed icon/text bars line up by default.
    pub(super) fn icon_edge(ui: &egui::Ui, configured: Option<u32>) -> f32 {
        match configured {
            Some(px) => px.clamp(16, 128) as f32,
            None => ui.spacing().interact_size.y.max(24.0),
        }
    }

    /// Paint one icon-faced hotbar button: allocated click rect + painter
    /// image (the codebase's sprite idiom — no egui Image widget), with
    /// optional label, solid border, dim tint, and countdown overlay.
    /// Also used by the hotbar editor's live preview.
    pub(super) fn draw_icon_button(
        ui: &mut egui::Ui,
        button: &crate::core::hotbar::ResolvedHotbarButton,
        texture: crate::frontend::gui::skin::SkinTexture,
        uv: egui::Rect,
        edge: f32,
    ) -> egui::Response {
        use crate::config::IconMode;

        let with_label = matches!(button.icon_mode, IconMode::IconAndLabel);

        // Label galley first so the allocation can fit icon + text.
        let label_galley = with_label.then(|| {
            let color = if button.dim {
                ui.visuals().weak_text_color()
            } else {
                button
                    .fg
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or_else(|| ui.visuals().text_color())
            };
            ui.painter().layout_no_wrap(
                button.label.clone(),
                egui::TextStyle::Button.resolve(ui.style()),
                color,
            )
        });
        let gap = 4.0;
        let width = edge
            + label_galley
                .as_ref()
                .map(|g| gap + g.size().x + gap)
                .unwrap_or(0.0);

        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, edge), egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let painter = ui.painter();

        // Button chrome: fill + hover highlight, matching egui's button feel.
        let visuals = ui.style().interact(&response);
        let fill = if button.dim {
            visuals.bg_fill
        } else {
            button
                .bg
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(visuals.bg_fill)
        };
        painter.rect_filled(rect, visuals.corner_radius, fill);

        // The icon cell, letterboxed square at the left edge.
        let icon_rect = egui::Rect::from_min_size(rect.min, egui::vec2(edge, edge));
        let tint = if button.dim {
            // Grayscale twin already applied; also fade it.
            egui::Color32::from_white_alpha(140)
        } else {
            egui::Color32::WHITE
        };
        painter.image(texture.texture, icon_rect.shrink(1.0), uv, tint);

        if let Some(galley) = label_galley {
            let pos = egui::pos2(
                rect.min.x + edge + gap,
                rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(pos, galley, ui.visuals().text_color());
        }

        // Border variant (barbar's c_HEX / cg_.. / bw_N, drawn not baked).
        if let Some(icon) = button.icon.as_ref() {
            if let Some(color) = icon.border.as_deref().and_then(parse_hex_color) {
                let bw = icon.border_width.unwrap_or(2).clamp(1, 10) as f32;
                match icon.border_end.as_deref().and_then(parse_hex_color) {
                    Some(end) => Self::paint_gradient_border(
                        painter,
                        icon_rect,
                        bw,
                        color,
                        end,
                        icon.border_dir,
                    ),
                    None => {
                        painter.rect_stroke(
                            icon_rect.shrink(bw / 2.0),
                            visuals.corner_radius,
                            egui::Stroke::new(bw, color),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
        }

        // Countdown overlay: bottom-center of the icon, barbar-style.
        if let Some(secs) = button.countdown_secs.filter(|s| *s > 0) {
            let text = format!("{}s", secs);
            let font = egui::TextStyle::Small.resolve(ui.style());
            let galley =
                painter.layout_no_wrap(text, font, egui::Color32::WHITE);
            let pos = egui::pos2(
                icon_rect.center().x - galley.size().x / 2.0,
                icon_rect.max.y - galley.size().y - 1.0,
            );
            // Scrim behind the digits so they read over any art.
            painter.rect_filled(
                egui::Rect::from_min_size(pos, galley.size()).expand(1.0),
                2.0,
                egui::Color32::from_black_alpha(160),
            );
            painter.galley(pos, galley, egui::Color32::WHITE);
        }

        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// Gradient position 0..1 at `pos` within `rect`, per barbar's cg
    /// direction formulas (horizontal px/w, diagonal averages, radial
    /// center distance, square Chebyshev distance).
    fn gradient_t(dir: crate::config::GradientDir, pos: egui::Pos2, rect: egui::Rect) -> f32 {
        use crate::config::GradientDir;
        let w = rect.width().max(1.0);
        let h = rect.height().max(1.0);
        let px = pos.x - rect.min.x;
        let py = pos.y - rect.min.y;
        let t = match dir {
            GradientDir::Horizontal => px / w,
            GradientDir::Vertical => py / h,
            GradientDir::DiagonalDown => (px / w + py / h) / 2.0,
            GradientDir::DiagonalUp => ((w - px) / w + py / h) / 2.0,
            GradientDir::Radial => {
                let c = rect.center();
                let max = (w * w + h * h).sqrt() / 2.0;
                pos.distance(c) / max.max(1.0)
            }
            GradientDir::Square => {
                let c = rect.center();
                ((pos.x - c.x).abs() / (w / 2.0)).max((pos.y - c.y).abs() / (h / 2.0))
            }
        };
        t.clamp(0.0, 1.0)
    }

    /// Two-color border drawn as short filled strips along the rect's four
    /// edges, each tinted by the gradient at its midpoint. Segments give
    /// uniform handling of all six directions (a mesh can't express the
    /// radial/square ones per-vertex).
    fn paint_gradient_border(
        painter: &egui::Painter,
        rect: egui::Rect,
        bw: f32,
        start: egui::Color32,
        end: egui::Color32,
        dir: crate::config::GradientDir,
    ) {
        const SEGMENTS: u32 = 16;
        let lerp = |t: f32| -> egui::Color32 {
            let a = egui::Rgba::from(start);
            let b = egui::Rgba::from(end);
            egui::Color32::from(a * (1.0 - t) + b * t)
        };
        let mut strip = |seg: egui::Rect| {
            painter.rect_filled(seg, 0.0, lerp(Self::gradient_t(dir, seg.center(), rect)));
        };
        let step = rect.width() / SEGMENTS as f32;
        for i in 0..SEGMENTS {
            let x0 = rect.min.x + i as f32 * step;
            let x1 = if i + 1 == SEGMENTS { rect.max.x } else { x0 + step };
            strip(egui::Rect::from_min_max(
                egui::pos2(x0, rect.min.y),
                egui::pos2(x1, rect.min.y + bw),
            ));
            strip(egui::Rect::from_min_max(
                egui::pos2(x0, rect.max.y - bw),
                egui::pos2(x1, rect.max.y),
            ));
        }
        // Side strips skip the corner rows the top/bottom already painted.
        let inner_h = (rect.height() - 2.0 * bw).max(0.0);
        let step = inner_h / SEGMENTS as f32;
        if step > 0.0 {
            for i in 0..SEGMENTS {
                let y0 = rect.min.y + bw + i as f32 * step;
                let y1 = if i + 1 == SEGMENTS {
                    rect.max.y - bw
                } else {
                    y0 + step
                };
                strip(egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, y0),
                    egui::pos2(rect.min.x + bw, y1),
                ));
                strip(egui::Rect::from_min_max(
                    egui::pos2(rect.max.x - bw, y0),
                    egui::pos2(rect.max.x, y1),
                ));
            }
        }
    }

    pub(super) fn render_performance_content(app_core: &AppCore, ui: &mut egui::Ui) {
        use crate::performance::{PerfFrontend, PerfMetric, PerfSeverity, PERF_METRICS};

        let cfg = app_core.perf_overlay_data(true);
        let stats = &app_core.perf_stats;

        // Rows derive from the shared metric table, filtered to what the
        // GUI actually records — a metric this frontend can't measure
        // never renders as a confident-looking zero.
        let visible: Vec<&PerfMetric> = PERF_METRICS
            .iter()
            .filter(|metric| metric.in_scope(PerfFrontend::Gui))
            .filter(|metric| metric.enabled_in(&cfg))
            .collect();

        if visible.is_empty() {
            ui.weak("All performance metrics are disabled in settings.");
            return;
        }

        // Keep the numbers live at ~1 Hz while the monitor is visible,
        // without repainting fast enough to distort what it measures.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));

        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("performance_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for metric in visible {
                    let severity = metric.severity.map(|f| f(stats));
                    let value_color = match severity {
                        Some(PerfSeverity::Crit) => egui::Color32::from_rgb(235, 90, 90),
                        Some(PerfSeverity::Warn) => egui::Color32::from_rgb(230, 175, 60),
                        _ => ui.visuals().text_color(),
                    };
                    let value = (metric.format)(stats);
                    let mut lines = value.lines();
                    let first = lines.next().unwrap_or("");
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{:<8}", metric.label))
                                .monospace()
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.label(RichText::new(first).monospace().color(value_color));
                        if cfg.sparklines {
                            if let Some(spark) = metric.spark {
                                Self::draw_perf_sparkline(ui, &spark(stats));
                            }
                        }
                    });
                    for line in lines {
                        ui.label(
                            RichText::new(format!("{:<8} {}", "", line))
                                .monospace()
                                .color(value_color),
                        );
                    }
                }
            });
    }

    /// Small trend polyline next to a performance row, normalized to the
    /// series max.
    fn draw_perf_sparkline(ui: &mut egui::Ui, values: &[f32]) {
        if values.len() < 2 {
            return;
        }
        let height = ui.text_style_height(&egui::TextStyle::Monospace).max(8.0);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(64.0, height), egui::Sense::hover());
        let max = values.iter().cloned().fold(0.0f32, f32::max);
        if max <= 0.0 {
            return;
        }
        let n = values.len();
        let points: Vec<egui::Pos2> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = rect.left() + rect.width() * i as f32 / (n - 1) as f32;
                let y = rect.bottom() - (v / max).clamp(0.0, 1.0) * (rect.height() - 1.0);
                egui::pos2(x, y)
            })
            .collect();
        ui.painter().add(egui::Shape::line(
            points,
            egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        ));
    }

    pub(super) fn render_dashboard_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        indicators: &[(String, u8)],
        data: Option<&crate::config::DashboardWidgetData>,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) {
        use crate::config::DashboardLayout;

        // Config-driven, matching the TUI: layout, spacing, hide_inactive.
        // Missing config = flow layout, default spacing, hide inactive.
        let layout = data
            .map(|d| DashboardLayout::from_str(&d.layout))
            .unwrap_or(DashboardLayout::Flow);
        let hide_inactive = data.map(|d| d.hide_inactive).unwrap_or(true);
        let spacing_chars = data.map(|d| d.spacing).unwrap_or(1);

        let now_server =
            chrono::Utc::now().timestamp() + app_core.message_processor.server_time_offset;

        // Candidate ids in config order (the authored set + arrangement),
        // then any runtime-only ids the server sent that the config omits.
        // A grouped/swapping cell (e.g. one POSTURE entry with per-posture
        // states) lives in the config with an id the server never flips, so
        // iterating the config — not just the runtime list — is what lets it
        // appear at all.
        let mut candidate_ids: Vec<String> = Vec::new();
        if let Some(d) = data {
            for def in &d.indicators {
                candidate_ids.push(def.id.clone());
            }
        }
        for (id, _) in indicators {
            if !candidate_ids.iter().any(|c| c.eq_ignore_ascii_case(id)) {
                candidate_ids.push(id.clone());
            }
        }

        // Stack-group tag per id (config only): entries sharing a non-empty
        // `stack` layer into one square. Case-insensitive lookup, empty = none.
        let stack_of = |id: &str| -> String {
            data.and_then(|d| {
                d.indicators
                    .iter()
                    .find(|def| def.id.eq_ignore_ascii_case(id))
                    .map(|def| def.stack.clone())
            })
            .unwrap_or_default()
        };

        // Resolve each candidate once. A layer is visible when hide_inactive is
        // off, OR its runtime value > 0, OR (for a states-driven layer) any
        // state currently matches — so a posture group shows whichever posture
        // is active even though its own id never gets a runtime value.
        struct Layer {
            id: String,
            value: u8,
            resolved: crate::core::conditions::ResolvedStatusArt,
            visible: bool,
        }
        // A cell is either one standalone layer or a stack group of layers,
        // all painted into the same square. Cells keep first-seen order.
        struct Cell {
            stack: String,
            layers: Vec<Layer>,
        }
        let mut cells: Vec<Cell> = Vec::new();
        for id in candidate_ids {
            let value = indicators
                .iter()
                .find(|(rid, _)| rid.eq_ignore_ascii_case(&id))
                .map(|(_, v)| *v)
                .unwrap_or(0);
            let resolved = app_core
                .indicator_template(&id)
                .filter(|t| !t.states.is_empty() || t.icon_ref.is_some())
                .map(|t| {
                    crate::core::conditions::resolve_status(
                        t,
                        value > 0,
                        &app_core.game_state,
                        now_server,
                        app_core.gameobj_data_cached(),
                    )
                })
                .unwrap_or_default();
            let visible = !hide_inactive || value > 0 || resolved.state_matched;
            let stack = stack_of(&id);
            let layer = Layer { id, value, resolved, visible };
            // Merge into an existing stack cell of the same (non-empty) name;
            // otherwise open a new cell.
            match cells
                .iter_mut()
                .find(|c| !stack.is_empty() && c.stack.eq_ignore_ascii_case(&stack))
            {
                Some(cell) => cell.layers.push(layer),
                None => cells.push(Cell { stack, layers: vec![layer] }),
            }
        }
        // Drop cells with no visible layer.
        cells.retain(|cell| cell.layers.iter().any(|l| l.visible));
        if cells.is_empty() {
            ui.weak("No active status.");
            return;
        }

        // Icons scale with the window's text size. Spacing (in "chars") maps
        // to a fraction of the icon size so it reads similarly to the TUI.
        let icon_side = (ui.text_style_height(&egui::TextStyle::Body) * 1.5).clamp(14.0, 64.0);
        let gap = (spacing_chars as f32) * icon_side * 0.35;

        // Paint one visible layer into `rect`. Returns true if it drew art (so
        // a stack can fall back to a text label only when nothing drew).
        let paint_layer = |ui: &mut egui::Ui, rect: Rect, layer: &Layer| -> bool {
            let id = layer.id.as_str();
            let value = layer.value.max(if layer.resolved.state_matched { 1 } else { 0 });
            let color = layer
                .resolved
                .color
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or_else(|| match value {
                    1 => Color32::from_rgb(0x55, 0xb8, 0x6c),
                    2 => Color32::from_rgb(0xff, 0x88, 0x00),
                    _ => Color32::from_rgb(0xcd, 0x4d, 0x4d),
                });
            let sprite = match &layer.resolved.icon {
                Some(icon) => skin_art.and_then(|art| art.resolve_icon_ref(icon, id)),
                None => skin_art.and_then(|art| art.icon(id)),
            };
            if let Some(sprite) = sprite {
                let dest = crate::frontend::gui::skin::icon_dest(&sprite, rect);
                crate::frontend::gui::skin::paint_icon(ui.painter(), dest, &sprite, Color32::WHITE);
                true
            } else if super::status_icons::supported(id) {
                super::status_icons::paint(ui.painter(), rect, id, color, ui.visuals().window_fill());
                true
            } else {
                false
            }
        };

        // One cell: allocate a square and paint every visible layer into it,
        // overlaid (authored art positions each within the square). A single
        // artless layer falls back to a text label, as before.
        let paint_cell = |ui: &mut egui::Ui, cell: &Cell| {
            let visible_layers: Vec<&Layer> = cell.layers.iter().filter(|l| l.visible).collect();
            let (rect, response) =
                ui.allocate_exact_size(Vec2::splat(icon_side), egui::Sense::hover());
            let mut drew_any = false;
            let mut names: Vec<String> = Vec::new();
            for layer in &visible_layers {
                if paint_layer(ui, rect, layer) {
                    drew_any = true;
                }
                names.push(super::status_icons::display_name(&layer.id));
            }
            if !drew_any {
                // No art resolved for any layer: text label of the first
                // visible layer's id (single-status cells keep the old look).
                if let Some(first) = visible_layers.first() {
                    let value = first.value.max(if first.resolved.state_matched { 1 } else { 0 });
                    let color = first
                        .resolved
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or_else(|| match value {
                            1 => Color32::from_rgb(0x55, 0xb8, 0x6c),
                            2 => Color32::from_rgb(0xff, 0x88, 0x00),
                            _ => Color32::from_rgb(0xcd, 0x4d, 0x4d),
                        });
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &first.id,
                        egui::FontId::proportional(icon_side * 0.5),
                        color,
                    );
                }
            }
            response.on_hover_text(names.join(", "));
        };

        ui.spacing_mut().item_spacing = Vec2::splat(gap);
        match layout {
            DashboardLayout::Horizontal => {
                ui.horizontal(|ui| {
                    for cell in &cells {
                        paint_cell(ui, cell);
                    }
                });
            }
            DashboardLayout::Flow => {
                ui.horizontal_wrapped(|ui| {
                    for cell in &cells {
                        paint_cell(ui, cell);
                    }
                });
            }
            DashboardLayout::Vertical => {
                ui.vertical(|ui| {
                    for cell in &cells {
                        paint_cell(ui, cell);
                    }
                });
            }
            DashboardLayout::Grid { cols, .. } => {
                let cols = cols.max(1);
                egui::Grid::new(ui.id().with("dashboard_grid"))
                    .spacing(Vec2::splat(gap))
                    .show(ui, |ui| {
                        for (index, cell) in cells.iter().enumerate() {
                            paint_cell(ui, cell);
                            if (index + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
            }
        }
    }

    /// Wrayth-style room window: one flowing block inside a single scroll
    /// area — the description runs straight into "You also see ...", then
    /// the players and exits lines follow, links clickable throughout.
    /// Every section takes its natural height, so a tall enough window
    /// shows everything without scrolling.
    pub(super) fn render_room_content(
        ui: &mut egui::Ui,
        room: &crate::data::RoomContent,
        show: (bool, bool, bool, bool), // desc, objs, players, exits
        scroll_id: &str,
        text_size: f32,
        font_id: &egui::FontId,
        interact_focus: Option<&str>, // exist id to draw the focus ring on
    ) -> Option<GuiLinkClick> {
        // Cheap Arc clone; deep-cloning Visuals per window per frame is not.
        let style = ui.style().clone();
        let visuals = &style.visuals;
        let mut clicked_link = None;
        let max_height = ui.available_height().max(1.0);
        let (show_desc, show_objs, show_players, show_exits) = show;

        let mut body: Vec<StyledLine> = Vec::new();
        if show_desc {
            body.extend(room.description.iter().cloned());
        }
        // Objects continue the description paragraph, as in Wrayth:
        // "...coats them.  You also see some cuirbouilli leather, ..."
        if show_objs {
            let mut objs = room.objects.iter().cloned();
            if let Some(first) = objs.next() {
                if let Some(last) = body.last_mut() {
                    last.segments.push(TextSegment {
                        text: "  ".to_string(),
                        ..Default::default()
                    });
                    last.segments.extend(first.segments);
                } else {
                    body.push(first);
                }
                body.extend(objs);
            }
        }
        if show_players {
            body.extend(room.players.iter().cloned());
        }
        if show_exits {
            body.extend(room.exits.iter().cloned());
        }

        // Interact-mode focus ring: paint the focused entity's link with the
        // selection background so keyboard focus is visible in the room text.
        if let Some(focus) = interact_focus {
            let sel = visuals.selection.bg_fill;
            let sel_hex = format!("#{:02x}{:02x}{:02x}", sel.r(), sel.g(), sel.b());
            for line in &mut body {
                for segment in &mut line.segments {
                    if segment
                        .link_data
                        .as_ref()
                        .is_some_and(|l| l.exist_id.trim_start_matches('#') == focus)
                    {
                        segment.bg = Some(sel_hex.clone());
                    }
                }
            }
        }

        egui::ScrollArea::vertical()
            .id_salt(format!("room_scroll_{}", scroll_id))
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                if !room.name.is_empty() {
                    // Explicit size: room names track the window's text size,
                    // not the Heading style (the title-bar size setting owns
                    // that).
                    ui.label(
                        RichText::new(&room.name)
                            .font(egui::FontId {
                                size: text_size + 2.0,
                                family: font_id.family.clone(),
                            })
                            .strong(),
                    );
                }
                for line in &body {
                    if let Some(link) =
                        Self::render_styled_line(ui, line, visuals, None, font_id, true, None)
                    {
                        clicked_link = Some(link);
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
                            format!("{} - {}", effect.text, time)
                        };
                        response.on_hover_text(hover);
                    }
                }
            });
    }

    pub(super) fn format_target_line(
        creature: &crate::core::state::Creature,
        target_cfg: &TargetListConfig,
        status_position: &str,
    ) -> String {
        // <crtrStatus> can report several statuses at once ("[stu,prn]");
        // the legacy text parse contributes at most one
        let statuses = creature.display_statuses();
        let status_tag = if statuses.is_empty() {
            None
        } else {
            let abbreviated: Vec<String> = statuses
                .iter()
                .map(|s| Self::status_abbreviation(s, target_cfg))
                .collect();
            Some(format!("[{}]", abbreviated.join(",")))
        };
        if let Some(status) = status_tag {
            if status_position.eq_ignore_ascii_case("start") {
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
        // Dead marker leads (reads "Name [ded] [prn]"), via the same abbrev map.
        if player.dead {
            statuses.push(format!("[{}]", Self::status_abbreviation("dead", target_cfg)));
        }
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

    pub(super) fn render_targets_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        window_name: &str,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;
        let target_cfg = &app_core.config.target_list;
        // Per-window options from the layout def (set in the window editor,
        // shared with the TUI).
        let (show_appendage_count, status_override) = match app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window_name)
        {
            Some(crate::config::WindowDef::Targets { data, .. }) => {
                (data.show_body_part_count, data.status_position.clone())
            }
            _ => (false, None),
        };
        let status_position = status_override
            .as_deref()
            .unwrap_or(target_cfg.status_position.as_str());
        let current_target =
            Self::normalize_entity_id(&app_core.game_state.target_list.current_target);
        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("targets_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                let mut body_part_count: u32 = 0;
                for creature in &app_core.game_state.room_creatures {
                    let creature_id = Self::normalize_entity_id(&creature.id);
                    // Hostile gate, matching Lich Creature.targets and the TUI
                    // widget: require a <crtrStatus> snapshot with hostile==1.
                    // Unknown hostility (flags: None) is excluded.
                    if !creature.flags.as_ref().is_some_and(|f| f.hostile) {
                        continue;
                    }
                    // Appendages are still counted for the footer even though
                    // valid_target? also filters them.
                    if creature.is_body_part() {
                        body_part_count += 1;
                    }
                    // Lich valid_target? filtering (dead/animated/appendage +
                    // configured excluded nouns), canonical on Creature so the
                    // TUI/GUI/web lists stay in sync.
                    if !creature.is_valid_target(&target_cfg.excluded_nouns) {
                        continue;
                    }

                    let display_text =
                        Self::format_target_line(creature, target_cfg, status_position);
                    let is_current = !current_target.is_empty() && creature_id == current_target;
                    // Color priority: current target, then boss tiers from
                    // <crtrStatus> (AscensionBoss/MiniBoss, then challenging)
                    let styled = if is_current {
                        RichText::new(format!("> {}", display_text))
                            .color(Color32::from_rgb(0x62, 0xcf, 0x79))
                    } else if let Some(color) = creature
                        .flags
                        .as_ref()
                        .and_then(|f| {
                            if f.is_boss() {
                                target_cfg.boss_color.as_deref()
                            } else if f.challenging {
                                target_cfg.challenging_color.as_deref()
                            } else {
                                None
                            }
                        })
                        .and_then(parse_hex_color)
                    {
                        RichText::new(display_text).color(color)
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
                if show_appendage_count && body_part_count > 0 {
                    ui.weak(format!("Appendages: {}", body_part_count));
                }
            });

        clicked_link
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
                    // Dead players render dim (dead_color); living players use
                    // the default label color.
                    let styled = match player
                        .dead
                        .then(|| target_cfg.dead_color.as_deref())
                        .flatten()
                        .and_then(parse_hex_color)
                    {
                        Some(color) => RichText::new(display_text).color(color),
                        None => RichText::new(display_text),
                    };
                    let response = ui
                        .add(egui::Label::new(styled).sense(egui::Sense::click()))
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
        timestamps: Option<crate::config::TimestampPosition>,
    ) -> f32 {
        // Same job builder as rendering, so measured heights match rendered
        // heights exactly (timestamps included).
        let built =
            Self::build_line_job(ctx, line, visuals, None, font_id, wrap_width, timestamps);
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
        let timestamps = content.show_timestamps.then_some(content.timestamp_position);
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
                        ctx, line, visuals, wrap_width, font_id, timestamps,
                    ));
                }
            }
        } else {
            cache.heights.clear();
            cache.heights.reserve(rendered_count);
            for line in content.lines.iter().skip(start) {
                cache.heights.push(Self::measure_line_height(
                    ctx, line, visuals, wrap_width, font_id, timestamps,
                ));
            }
        }
        cache.wrap_width = wrap_width;
        cache.font_id = font_id.clone();
        cache.generation = content.generation;
        debug_assert_eq!(cache.heights.len(), rendered_count);
    }

    /// The command input line, rendered wherever its window is docked (or
    /// in the fallback bottom panel). Render paths are `&self`, so buffer
    /// edits and key events are stashed as a `CommandInputEcho` in egui
    /// temp data and drained once per frame by the app update loop.
    pub(super) fn render_command_input_widget(ui: &mut egui::Ui, seed: &str, drag_gutter: bool) {
        // Copy/Cut priority: when a game-window buffer selection is active, that
        // selection owns the clipboard, not this (often focused-but-unselected)
        // input. The focused TextEdit would otherwise consume the Copy event
        // during ui.add() and win the clipboard whenever the input held text.
        // Removing the Copy/Cut event here -- before the edit, regardless of
        // which window renders first -- lets the owning text window's buffer
        // copy path be the sole handler. With no active selection the event is
        // left in place, so copying from the input still works (bug #3).
        if Self::active_buffer_selection_present(ui.ctx()) {
            ui.ctx().input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Copy | egui::Event::Cut));
            });
        }

        let mut text = seed.to_string();
        let mut echo = CommandInputEcho::default();
        // Vertically center the single-line edit in whatever height the
        // window gives it.
        let edit_height = ui.text_style_height(&egui::TextStyle::Body) + 8.0;
        let pad = ((ui.available_height() - edit_height) / 2.0).max(0.0);
        if pad > 0.0 {
            ui.add_space(pad);
        }
        let edit = |ui: &mut egui::Ui, text: &mut String| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(egui::Id::new(COMMAND_INPUT_EDIT_ID))
                    .hint_text("Enter command...")
                    .desired_width(ui.available_width()),
            )
        };
        let response = if drag_gutter {
            // Title bar hidden: the TextEdit owns every drag in the body,
            // so this grip is the window's only drag surface. It is
            // hover-only on purpose — drags on it fall through to the
            // window body and move it.
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::Vec2::new(12.0, edit_height),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(rect) {
                    let color = ui.visuals().weak_text_color();
                    let center = rect.center();
                    for row in -1..=1i32 {
                        for col in 0..2i32 {
                            let pos = center
                                + egui::vec2(col as f32 * 4.0 - 2.0, row as f32 * 5.0);
                            ui.painter().circle_filled(pos, 1.2, color);
                        }
                    }
                }
                edit(ui, &mut text)
            })
            .inner
        } else {
            edit(ui, &mut text)
        };
        // Which keys drive submit/history/clear-line comes from the keybind
        // config (stashed each frame by stash_command_input_keys), so rebinding
        // works; the defaults Enter/↑/↓ are always included there too.
        let keys = ui
            .ctx()
            .data(|data| data.get_temp::<super::CommandInputKeys>(super::CommandInputKeys::id()))
            .unwrap_or_else(|| super::CommandInputKeys {
                submit: vec![egui::Key::Enter],
                history_prev: vec![egui::Key::ArrowUp],
                history_next: vec![egui::Key::ArrowDown],
                clear_line: Vec::new(),
            });

        let pressed_submit = ui.input(|i| keys.submit.iter().any(|k| i.key_pressed(*k)));
        if response.lost_focus() && pressed_submit {
            echo.submit = true;
            response.request_focus();
        }
        // History browsing + clear-line. consume_key keeps these keys from
        // reaching anything else while the input has focus.
        if response.has_focus() {
            let up = keys
                .history_prev
                .iter()
                .any(|k| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, *k)));
            let down = keys
                .history_next
                .iter()
                .any(|k| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, *k)));
            let clear = keys
                .clear_line
                .iter()
                .any(|(k, m)| ui.input_mut(|i| i.consume_key(*m, *k)));
            if clear {
                text.clear();
                echo.text = Some(String::new());
            } else if up {
                echo.history_prev = true;
            } else if down {
                echo.history_next = true;
            }
        }
        if text != seed {
            echo.text = Some(text);
        }
        if !echo.is_empty() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(CommandInputEcho::id(), echo));
        }
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
        // their exact place with no one-frame flicker. At the bottom this is
        // a no-op: the area's stuck-to-end flag re-pins the offset to the
        // end regardless of the stored value. The area id comes from last
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
                    let dropped_px: f32 = cache.heights[..drop_front]
                        .iter()
                        .map(|h| h + outer_spacing_y)
                        .sum();
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

        let mut scroll_area = if wrap {
            egui::ScrollArea::vertical()
        } else {
            egui::ScrollArea::both()
        };

        // Programmatic scroll (page keys / controller). egui's private
        // stuck-to-end flag only clears on USER input — a one-frame
        // explicit offset snaps back to the bottom next frame. So while a
        // key/pad scroll has us paged up, we HOLD the offset by
        // re-applying it every frame, and release the hold when the user
        // touches the wheel/drag, reaches the bottom, or presses End.
        let pending_key = egui::Id::new(("text_scroll_pending", scroll_id));
        let hold_key = egui::Id::new(("text_scroll_hold", scroll_id));
        let pending: Option<(u8, f32)> = outer_ctx.data_mut(|data| {
            let value = data.get_temp(pending_key);
            if value.is_some() {
                data.remove::<(u8, f32)>(pending_key);
            }
            value
        });
        let mut hold: Option<f32> = outer_ctx.data_mut(|data| data.get_temp(hold_key));

        // The user's own scroll input takes over instantly.
        let user_scrolled = ui.input(|input| {
            input.raw.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::MouseWheel { .. } | egui::Event::PointerButton { pressed: true, .. }
                )
            })
        });
        if user_scrolled {
            hold = None;
        }

        if let Some((kind, value)) = pending {
            let current = hold.or_else(|| {
                outer_ctx
                    .data_mut(|data| data.get_temp::<egui::Id>(area_id_key))
                    .and_then(|area_id| egui::scroll_area::State::load(&outer_ctx, area_id))
                    .map(|state| state.offset.y)
            });
            hold = match kind {
                1 => Some(0.0),      // home
                2 => None,           // end: drop the hold, stickiness resumes
                3 => {
                    // Absolute: scroll so buffer line `value` sits near the top.
                    // The height cache covers only the rendered tail (lines
                    // `start..`), so map the absolute index into it and sum the
                    // preceding rows' strides. Out-of-range clamps to the ends.
                    let target_line = value as usize;
                    let offset = if target_line < start {
                        Some(0.0)
                    } else {
                        let rendered_idx = target_line - start;
                        let cache = cache_handle.lock().expect("row height cache poisoned");
                        if rendered_idx >= cache.heights.len() {
                            None // past the cached tail → fall through to end
                        } else {
                            Some(
                                cache.heights[..rendered_idx]
                                    .iter()
                                    .map(|h| h + outer_spacing_y)
                                    .sum::<f32>()
                                    .max(0.0),
                            )
                        }
                    };
                    offset
                }
                _ => Some((current.unwrap_or(0.0) + value).max(0.0)),
            };
            if hold.is_none() {
                // Nudge to the bottom so stick_to_bottom re-engages.
                scroll_area = scroll_area.vertical_scroll_offset(f32::MAX / 4.0);
            }
        }
        if let Some(target) = hold {
            scroll_area = scroll_area.vertical_scroll_offset(target);
        }

        let output = scroll_area
            .id_salt(format!("text_scroll_{}", scroll_id))
            .stick_to_bottom(true)
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
                            cache.heights.iter().map(|h| h + spacing_y).sum();
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
                            for (i, h) in cache.heights.iter().enumerate() {
                                if pos.y < y + h + spacing_y || i == rendered_count - 1 {
                                    slot = i;
                                    slot_top = y;
                                    break;
                                }
                                y += h + spacing_y;
                            }
                            let line_index = start + slot;
                            let line_job = Self::build_line_job(
                                &ctx,
                                &content.lines[line_index],
                                &visuals,
                                search_query,
                                font_id,
                                wrap_width,
                                timestamps,
                            );
                            let galley = ctx.fonts_mut(|fonts| fonts.layout_job(line_job.job));
                            // Centered/right rows paint their galley offset
                            // within the full-width row; mirror that offset
                            // when mapping the pointer back to a character.
                            let drag_h_offset = if h_align != 0 && wrap_width.is_finite() {
                                let free = (wrap_width - galley.size().x).max(0.0);
                                if h_align == 1 { free / 2.0 } else { free }
                            } else {
                                0.0
                            };
                            let local = egui::Vec2::new(
                                pos.x - content_left - drag_h_offset,
                                pos.y - slot_top,
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

                    let line_job = Self::build_line_job(
                        &ctx,
                        line,
                        &visuals,
                        search_query,
                        font_id,
                        wrap_width,
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
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, height), sense);
                    let h_offset = match h_align {
                        1 => ((rect.width() - galley_size.x) / 2.0).max(0.0),
                        2 => (rect.width() - galley_size.x).max(0.0),
                        _ => 0.0,
                    };
                    let galley_pos = rect.left_top() + Vec2::new(h_offset, 0.0);
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
                let bottom_space: f32 = cache.heights[last_visible..]
                    .iter()
                    .map(|h| h + spacing_y)
                    .sum();
                if bottom_space > spacing_y {
                    ui.allocate_space(Vec2::new(1.0, bottom_space - spacing_y));
                }
            });
        // Next frame's anchoring pre-pass targets this area's real id.
        outer_ctx.data_mut(|data| data.insert_temp(area_id_key, output.id));

        // Settle the programmatic hold against the real layout: clamp to
        // the actual max offset, and release it once we're at the bottom
        // so stick-to-bottom auto-scroll resumes.
        let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
        let settled = hold.map(|h| h.min(max_offset)).filter(|h| *h < max_offset - 4.0);
        outer_ctx.data_mut(|data| match settled {
            Some(value) => {
                data.insert_temp(hold_key, value);
            }
            None => {
                data.remove::<f32>(hold_key);
            }
        });

        // Re-arm stick-to-bottom when a user scroll settles just shy of the
        // end. egui only re-sticks on EXACT offset equality, and scrollbar
        // drags / kinetic flicks routinely stop a fraction of a row short —
        // visually "at the bottom" but unstuck, so incoming text left the
        // view trailing slightly. When the scroll is at rest (no button
        // held, not moving up, no programmatic hold) within one row of the
        // end, snap to it; egui's equality check then re-sticks next frame.
        let prev_offset_key = egui::Id::new(("text_scroll_prev_offset", scroll_id));
        let prev_offset = outer_ctx.data_mut(|data| {
            let prev = data.get_temp::<f32>(prev_offset_key);
            data.insert_temp(prev_offset_key, output.state.offset.y);
            prev
        });
        let snap_tolerance =
            outer_ctx.fonts_mut(|fonts| fonts.row_height(font_id)) + outer_spacing_y;
        let shy_of_bottom = max_offset - output.state.offset.y;
        let moving_up = prev_offset.is_some_and(|prev| output.state.offset.y < prev - 0.1);
        let pointer_down = outer_ctx.input(|i| i.pointer.any_down());
        if settled.is_none()
            && shy_of_bottom > 0.0
            && shy_of_bottom <= snap_tolerance
            && !moving_up
            && !pointer_down
        {
            if let Some(mut state) = egui::scroll_area::State::load(&outer_ctx, output.id) {
                state.offset.y = max_offset;
                state.store(&outer_ctx, output.id);
                outer_ctx.request_repaint();
            }
        }

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

        // Skin background: reserve a paint slot now (so the art stays behind
        // the content), fill it after layout from the content's real extent.
        // Compact one-row widgets live in auto-sized windows whose pre-layout
        // available rect can be taller than the final frame; painting that
        // rect up front spilled the art below the window.
        let background_slot = settings.background.clone().map(|background| {
            (
                ui.painter().add(egui::Shape::Noop),
                ui.available_rect_before_wrap(),
                background,
            )
        });

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

        let clicked_link = match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Reserve(content)
            | WindowContent::Spells(content) => {
                let query = Self::active_search_query(app_core);
                Self::render_text_content(
                    ui,
                    content,
                    &tab.window_name,
                    query.as_deref(),
                    &font_id,
                    settings.wrap_text,
                    window.content_align.as_deref(),
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
            WindowContent::Compass(compass) => {
                Self::render_compass_content(app_core, ui, compass, settings.skin_art.as_deref())
            }
            WindowContent::Map(map_data) => {
                Self::render_map_content(app_core, ui, map_data, settings.map_zoom)
            }
            WindowContent::Hand { item, link } => {
                let hand_prefix = if window.name.to_ascii_lowercase().contains("left") {
                    "L"
                } else if window.name.to_ascii_lowercase().contains("right") {
                    "R"
                } else {
                    "S"
                };
                // Status-driven icon states from the window's layout def.
                let resolved = app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::Hand { data, .. } => Some(data),
                        _ => None,
                    })
                    .filter(|data| !data.states.is_empty())
                    .map(|data| {
                        let now_server = chrono::Utc::now().timestamp()
                            + app_core.message_processor.server_time_offset;
                        crate::core::conditions::resolve_hand(
                            data,
                            &app_core.game_state,
                            now_server,
                            app_core.gameobj_data_cached(),
                        )
                    })
                    .unwrap_or_default();
                Self::render_hand_content(
                    ui,
                    hand_prefix,
                    item,
                    link,
                    settings.skin_art.as_deref(),
                    &resolved,
                    settings.hand_icon_size,
                )
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
                        window.content_align.as_deref(),
                    ) {
                        clicked_link.get_or_insert(link);
                    }
                } else {
                    ui.label("No active tab content.");
                }
                clicked_link
            }
            WindowContent::Room(room) => {
                // Per-window section toggles from the layout def (set in the
                // window editor, shared with the TUI). The room-name heading
                // is always shown: the def's show_name flag drives the TUI
                // border title, which has no GUI equivalent.
                let show = match app_core
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.name() == tab.window_name)
                {
                    Some(crate::config::WindowDef::Room { data, .. }) => (
                        data.show_desc,
                        data.show_objs,
                        data.show_players,
                        data.show_exits,
                    ),
                    _ => (true, true, true, true),
                };
                let interact_focus = app_core.interact_focus_exist_id();
                Self::render_room_content(
                    ui,
                    room,
                    show,
                    &tab.window_name,
                    text_size,
                    &font_id,
                    interact_focus.as_deref(),
                )
            }
            WindowContent::ActiveEffects(content) => {
                Self::render_active_effects_content(ui, content, settings);
                None
            }
            WindowContent::WebUi(content) => {
                Self::render_webui_content(ui, content);
                None
            }
            WindowContent::Targets => {
                Self::render_targets_content(app_core, ui, &tab.window_name)
            }
            WindowContent::Players => Self::render_players_content(app_core, ui),
            WindowContent::Countdown(countdown) => {
                Self::render_countdown_content(app_core, ui, countdown, &settings);
                None
            }
            WindowContent::Indicator(indicator) => {
                // Per-indicator gray override wins over the global toggle.
                let gray = settings
                    .gray_icon_overrides
                    .get(&indicator.indicator_id)
                    .or_else(|| {
                        settings
                            .gray_icon_overrides
                            .get(&indicator.indicator_id.to_ascii_uppercase())
                    })
                    .copied()
                    .unwrap_or(settings.gray_inactive_icons);
                // Resolve the status template's condition-driven art (state
                // icon/color) from the cached templates; empty when the id has
                // no template or no states (falls back to id-keyed art).
                let resolved = app_core
                    .indicator_template(&indicator.indicator_id)
                    .filter(|t| !t.states.is_empty() || t.icon_ref.is_some())
                    .map(|template| {
                        let now_server = chrono::Utc::now().timestamp()
                            + app_core.message_processor.server_time_offset;
                        crate::core::conditions::resolve_status(
                            template,
                            indicator.active,
                            &app_core.game_state,
                            now_server,
                            app_core.gameobj_data_cached(),
                        )
                    })
                    .unwrap_or_default();
                Self::render_indicator_content(
                    ui,
                    &tab.id.title,
                    indicator,
                    settings.skin_art.as_deref(),
                    gray,
                    &resolved,
                );
                None
            }
            WindowContent::InjuryDoll(doll) => {
                // Resolve the palette from this doll's config (per-level
                // injury*_color/scar*_color overrides), matching the TUI —
                // the GUI used to ignore these and hardcode the palette.
                let palette = app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::InjuryDoll { data, .. } => {
                            Some(Self::resolved_injury_palette(data))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(Self::default_injury_palette);
                Self::render_injury_doll(
                    ui,
                    &doll.injuries,
                    settings.skin_art.as_deref(),
                    settings.doll_grayscale,
                    &palette,
                );
                None
            }
            WindowContent::Dashboard { indicators } => {
                // Read this dashboard's config (layout/spacing/hide_inactive +
                // per-id icon/colors via the status templates), matching the
                // TUI. Missing config falls back to flow + hide-inactive.
                let data = app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::Dashboard { data, .. } => Some(data.clone()),
                        _ => None,
                    });
                Self::render_dashboard_content(
                    app_core,
                    ui,
                    indicators,
                    data.as_ref(),
                    settings.skin_art.as_deref(),
                );
                None
            }
            WindowContent::GS4Experience => {
                Self::render_gs4_experience_content(app_core, ui, &tab.window_name, &settings);
                None
            }
            WindowContent::Experience => {
                Self::render_dr_experience_content(app_core, ui);
                None
            }
            WindowContent::Encumbrance => {
                Self::render_encumbrance_content(app_core, ui, &tab.window_name, &settings);
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
                Self::render_container_content(app_core, ui, container_title, settings.wrap_text)
            }
            WindowContent::DialogPanel { dialog_id } => {
                Self::render_dialog_panel_content(
                    app_core,
                    ui,
                    dialog_id,
                    settings.skin_art.as_deref(),
                );
                None
            }
            WindowContent::Quickbar => Self::render_quickbar_content(app_core, ui),
            WindowContent::Hotkeybar { bar } => Self::render_hotkeybar_content(
                app_core,
                ui,
                &window.name,
                bar,
                settings.skin_art.as_deref(),
            ),
            WindowContent::Performance => {
                Self::render_performance_content(app_core, ui);
                None
            }
            WindowContent::CommandInput { .. } => {
                Self::render_command_input_widget(
                    ui,
                    settings.command_input_seed.as_deref().unwrap_or(""),
                    settings.command_input_drag_gutter,
                );
                None
            }
            WindowContent::Empty => {
                // Spacers reserve their area and draw nothing.
                ui.allocate_space(ui.available_size());
                None
            }
        };

        if let Some((slot, avail, background)) = background_slot {
            let mut rect = avail;
            if Self::is_compact_center_widget(&window.widget_type) {
                // One-row widgets: hug the rendered content so the art can't
                // run past an auto-shrunk frame.
                rect.max.y = rect.max.y.min(ui.min_rect().max.y);
            }
            let shapes = crate::frontend::gui::skin::background_shapes(
                rect,
                &background,
                ui.visuals().window_fill(),
            );
            ui.painter()
                .with_clip_rect(rect)
                .set(slot, egui::Shape::Vec(shapes));
        }

        clicked_link
    }
}

/// One rendered line: its composed layout job plus the char ranges of its
/// clickable links within that composed text.
pub(super) struct GuiLineJob {
    job: egui::text::LayoutJob,
    links: Vec<(std::ops::Range<usize>, LinkData)>,
    /// Custom-emoji image slots as `(char_start, char_end, name)` over the
    /// `:name:` fallback text kept in the job. The caller paints the image over
    /// this run after the galley is drawn (see `paint_custom_emoji_runs`).
    custom_runs: Vec<(usize, usize, String)>,
    /// Minimum row height this line needs so an oversized custom emoji (size
    /// knob > 1) isn't clipped by the line above/below. 0.0 when the line has
    /// no emoji taller than the text.
    min_height: f32,
}

/// Buffer-anchored text selection for virtualized text windows. Endpoints
/// address (line uid, char index) in the stream itself — uid resolves back
/// to a buffer index through the window's generation counter — so the
/// selection survives scrolling, stick-to-bottom shifts, and buffer trims,
/// and Ctrl+C can copy lines that are no longer on screen.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct GuiBufferSelection {
    scroll_id: String,
    /// Where the selection started (press point).
    anchor: (u64, usize),
    /// The moving end; follows the pointer while dragging.
    head: (u64, usize),
    dragging: bool,
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
    use super::{GuiBufferSelection, VellumGuiApp};

    #[test]
    fn countdown_remaining_clamps_to_zero_when_elapsed() {
        // now = 150_000ms (150s), end = 100s -> elapsed
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(100, 0, 150_000), 0);
    }

    #[test]
    fn countdown_remaining_counts_down_from_end_time() {
        // now = 100_000ms (100s), end = 110s -> exactly 10s left
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 100_000), 10);
    }

    #[test]
    fn countdown_remaining_applies_server_offset() {
        // Server clock runs 5s ahead of local time. now = 100s -> 5s left.
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 5, 100_000), 5);
    }

    #[test]
    fn countdown_remaining_ceilings_partial_seconds() {
        // 1001ms remaining -> displays 2 (ceiling): end 110s, now 108_999ms
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 108_999), 2);
        // Exactly 1000ms remaining -> displays 1
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 109_000), 1);
        // 1ms remaining -> still displays 1
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 109_999), 1);
        // 0ms remaining -> displays 0
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 110_000), 0);
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
        // Server clock runs 5s ahead of local time: end 110s, now 100s,
        // offset +5 -> 110 - (100 + 5) = 5.0.
        assert_eq!(
            VellumGuiApp::countdown_remaining_seconds_f(110, 5, 100.0),
            5.0
        );
    }

    #[test]
    fn countdown_remaining_fraction_matches_number_sign() {
        // Regression: the fractional bar and the whole-second number must use
        // the SAME offset sign, or they disagree on a drifted clock. With a
        // non-symmetric offset the two must still describe the same remaining
        // time. end=120s, now=100s (=100_000ms), offset=+3 -> 17s remaining.
        let f = VellumGuiApp::countdown_remaining_seconds_f(120, 3, 100.0);
        let n = VellumGuiApp::countdown_remaining_seconds(120, 3, 100_000);
        assert_eq!(f, 17.0);
        assert_eq!(n, 17); // number ceilings, but here it's a whole value
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
    fn word_char_range_expands_around_word_chars() {
        assert_eq!(VellumGuiApp::word_char_range("you say hello", 5), (4, 7));
        // Punctuation selects just itself.
        assert_eq!(VellumGuiApp::word_char_range("a, b", 1), (1, 2));
        // Clamps past-the-end to the last char.
        assert_eq!(VellumGuiApp::word_char_range("word", 99), (0, 4));
        assert_eq!(VellumGuiApp::word_char_range("", 0), (0, 0));
        // Char (not byte) indexing with multibyte text.
        assert_eq!(VellumGuiApp::word_char_range("éléphant rose", 2), (0, 8));
    }

    #[test]
    fn slice_line_by_chars_uses_char_offsets() {
        assert_eq!(
            VellumGuiApp::slice_line_by_chars("hello world", Some(6), None),
            "world"
        );
        assert_eq!(
            VellumGuiApp::slice_line_by_chars("hello world", None, Some(5)),
            "hello"
        );
        // Multibyte chars: offsets count chars, not bytes.
        assert_eq!(
            VellumGuiApp::slice_line_by_chars("éé abc", Some(3), Some(6)),
            "abc"
        );
        // Reversed offsets are reordered, out-of-range clamps to the end.
        assert_eq!(
            VellumGuiApp::slice_line_by_chars("abc", Some(99), Some(1)),
            "bc"
        );
    }

    #[test]
    fn resolve_line_uid_clamps_trimmed_and_overrun() {
        // base 100, 10 lines: uids 100..110 are live.
        assert_eq!(VellumGuiApp::resolve_line_uid(100, 10, 105), 5);
        // Trimmed off the front (uid below base) clamps to the first line.
        assert_eq!(VellumGuiApp::resolve_line_uid(100, 10, 95), 0);
        // Past the end clamps to the last line.
        assert_eq!(VellumGuiApp::resolve_line_uid(100, 10, 500), 9);
        // Wrapping base (fresh buffer populated without generation bumps).
        let base = 0u64.wrapping_sub(3);
        assert_eq!(VellumGuiApp::resolve_line_uid(base, 5, base.wrapping_add(4)), 4);
    }

    #[test]
    fn ordered_selection_endpoints_orders_reversed_drags() {
        let selection = GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (107, 4),
            head: (103, 2),
            dragging: false,
        };
        assert_eq!(
            VellumGuiApp::ordered_selection_endpoints(&selection, 100, 10),
            ((3, 2), (7, 4))
        );
        // Same line, chars reversed.
        let selection = GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (105, 9),
            head: (105, 2),
            dragging: false,
        };
        assert_eq!(
            VellumGuiApp::ordered_selection_endpoints(&selection, 100, 10),
            ((5, 2), (5, 9))
        );
    }

    #[test]
    fn buffer_selection_copy_text_spans_lines_and_slices_endpoints() {
        use crate::data::StyledLine;
        let mut content = crate::data::TextContent::new("Test", 100);
        content.add_line(StyledLine::from_text("first line"));
        content.add_line(StyledLine::from_text("second line"));
        content.add_line(StyledLine::from_text("third line"));
        let base = content.generation.wrapping_sub(content.lines.len() as u64);

        // From "line" on the first line through "third" on the last.
        let selection = GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (base, 6),
            head: (base.wrapping_add(2), 5),
            dragging: false,
        };
        assert_eq!(
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &selection, base, None),
            "line\nsecond line\nthird"
        );

        // Reversed drag yields the same text.
        let reversed = GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (base.wrapping_add(2), 5),
            head: (base, 6),
            dragging: false,
        };
        assert_eq!(
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &reversed, base, None),
            "line\nsecond line\nthird"
        );

        // Single-line slice.
        let single = GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (base.wrapping_add(1), 7),
            head: (base.wrapping_add(1), 11),
            dragging: false,
        };
        assert_eq!(
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &single, base, None),
            "line"
        );
    }

    #[test]
    fn buffer_selection_copy_text_survives_front_trim() {
        use crate::data::StyledLine;
        let mut content = crate::data::TextContent::new("Test", 3);
        for i in 0..5 {
            content.add_line(StyledLine::from_text(format!("line {}", i)));
        }
        // Buffer now holds lines 2..5; generation is 5.
        let base = content.generation.wrapping_sub(content.lines.len() as u64);
        // Anchor on a line that has been trimmed away clamps to the first
        // remaining line.
        let selection = GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (0, 0),
            head: (base.wrapping_add(1), 6),
            dragging: false,
        };
        assert_eq!(
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &selection, base, None),
            "line 2\nline 3"
        );
    }

    #[test]
    fn build_line_job_records_link_char_ranges() {
        use crate::data::{LinkData, StyledLine, TextSegment};
        let line = StyledLine {
            segments: vec![
                TextSegment::plain("héllo "),
                TextSegment {
                    text: "an orc".into(),
                    link_data: Some(LinkData {
                        exist_id: "123".into(),
                        noun: "orc".into(),
                        text: "an orc".into(),
                        coord: None,
                    }),
                    ..Default::default()
                },
                TextSegment::plain(" lunges!"),
            ],
            stream: "main".into(),
            timestamp: None,
        };
        let visuals = eframe::egui::Visuals::default();
        let font_id = eframe::egui::FontId::monospace(14.0);
        let built =
            VellumGuiApp::build_line_job(&eframe::egui::Context::default(), &line, &visuals, None, &font_id, f32::INFINITY, None);
        assert_eq!(built.job.text, "héllo an orc lunges!");
        assert_eq!(built.links.len(), 1);
        // Char (not byte) range: "héllo " is 6 chars.
        assert_eq!(built.links[0].0, 6..12);
        assert_eq!(built.links[0].1.exist_id, "123");
    }

    #[test]
    fn build_line_job_records_custom_emoji_runs() {
        use crate::core::custom_emoji::{self, CustomEmoji, CustomEmojiRegistry, EmojiFormat};
        use crate::data::{StyledLine, TextSegment};

        // Write a real 1x1 PNG so is_paintable's decode succeeds.
        let tmp = std::env::temp_dir().join(format!("vellum_emoji_bl_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("vibecat.png");
        {
            use image::ImageEncoder;
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
                .unwrap();
            std::fs::write(&path, png).unwrap();
        }
        let mut reg = CustomEmojiRegistry::default();
        reg.insert_for_test(CustomEmoji {
            name: "vibecat".into(),
            path,
            format: EmojiFormat::Png,
        });
        custom_emoji::set_for_test(reg);

        // A line with a tagged custom-emoji segment, as the resolver produces.
        let line = StyledLine {
            segments: vec![
                TextSegment::plain("yep "),
                TextSegment {
                    text: ":vibecat:".into(),
                    custom_emoji: Some("vibecat".into()),
                    ..Default::default()
                },
            ],
            stream: "main".into(),
            timestamp: None,
        };
        let visuals = eframe::egui::Visuals::default();
        let font_id = eframe::egui::FontId::monospace(14.0);
        let ctx = eframe::egui::Context::default();
        ctx.begin_pass(eframe::egui::RawInput::default());
        let built =
            VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, f32::INFINITY, None);
        let _ = ctx.end_pass();

        // A paintable custom emoji occupies a space-run placeholder (not the
        // wide `:name:` text), and the run is recorded over exactly it.
        ctx.begin_pass(eframe::egui::RawInput::default());
        let placeholder = VellumGuiApp::emoji_placeholder(&ctx, &font_id);
        let _ = ctx.end_pass();
        let ph = placeholder.chars().count();
        assert!(ph >= 1, "placeholder is at least one space");
        let expected = format!("yep {placeholder}");
        assert_eq!(built.job.text, expected);
        assert_eq!(built.custom_runs.len(), 1, "must record the emoji slot");
        assert_eq!(built.custom_runs[0].0, 4);
        assert_eq!(built.custom_runs[0].1, 4 + ph, "run spans the placeholder");
        assert_eq!(built.custom_runs[0].2, "vibecat");

        // compose_line_text must agree so copy/selection offsets stay aligned.
        assert_eq!(
            VellumGuiApp::compose_line_text(&ctx, &font_id, &line, None),
            expected
        );

        // At the default size (1.0) the line needs no extra height...
        super::custom_emoji_render::set_geometry(1.0, 0.2);
        let a = VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, f32::INFINITY, None);
        assert_eq!(a.min_height, 0.0);
        // ...but an oversized emoji grows the row so it isn't clipped.
        super::custom_emoji_render::set_geometry(2.0, 0.2);
        let b = VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, f32::INFINITY, None);
        assert!(b.min_height > 0.0, "size>1 must set a taller min_height");
        super::custom_emoji_render::set_geometry(1.0, 0.2); // reset

        // The slot (row_height * width_factor) must be WIDER than the emoji
        // square (row_height * size_factor) so there's positive padding split
        // symmetrically on both sides — pos_from_cursor can't be trusted for
        // the width (it ignores extra_letter_spacing), so the painter uses
        // width_factor directly. Guard the invariant that keeps padding > 0.
        let size = super::custom_emoji_render::size_factor();
        let width_factor = super::custom_emoji_render::width_factor();
        assert!(
            width_factor >= size,
            "reserved width ({width_factor}) must be >= the square ({size}) so padding is never negative"
        );
        // With the default 0.2 spacing there is strictly positive padding.
        assert!(width_factor > size, "default spacing gives positive padding");

        custom_emoji::set_for_test(CustomEmojiRegistry::default());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn compose_line_text_matches_job_text() {
        use crate::data::{StyledLine, TextSegment};
        let line = StyledLine {
            segments: vec![TextSegment::plain("a"), TextSegment::plain("bc")],
            stream: "main".into(),
            timestamp: None,
        };
        let visuals = eframe::egui::Visuals::default();
        let font_id = eframe::egui::FontId::monospace(14.0);
        let built =
            VellumGuiApp::build_line_job(&eframe::egui::Context::default(), &line, &visuals, None, &font_id, f32::INFINITY, None);
        assert_eq!(VellumGuiApp::compose_line_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &line, None), built.job.text);
    }

    #[test]
    fn injury_level_color_distinguishes_injuries_from_scars() {
        use eframe::egui::Color32;
        let palette = VellumGuiApp::default_injury_palette();
        assert_eq!(
            VellumGuiApp::injury_level_color(&palette, 0),
            Color32::from_rgb(0x33, 0x33, 0x33)
        );
        assert_eq!(
            VellumGuiApp::injury_level_color(&palette, 3),
            Color32::from_rgb(0xff, 0x00, 0x00)
        );
        assert_eq!(
            VellumGuiApp::injury_level_color(&palette, 6),
            Color32::from_rgb(0x55, 0x55, 0x55)
        );
        // Out-of-range levels clamp to the deepest scar color.
        assert_eq!(
            VellumGuiApp::injury_level_color(&palette, 9),
            VellumGuiApp::injury_level_color(&palette, 6)
        );
    }

    #[test]
    fn resolved_injury_palette_honors_config_overrides() {
        use eframe::egui::Color32;
        let data = crate::config::InjuryDollWidgetData {
            injury1_color: Some("#00ff00".to_string()),
            ..Default::default()
        };
        let palette = VellumGuiApp::resolved_injury_palette(&data);
        // The overridden level renders the user's color...
        assert_eq!(palette[1], Color32::from_rgb(0x00, 0xff, 0x00));
        // ...while un-overridden levels keep the shared defaults.
        assert_eq!(palette[3], Color32::from_rgb(0xff, 0x00, 0x00));
    }

    // --- Keybind bug #3: copy priority. A non-empty game-window selection owns
    // Copy/Cut over the command input; a collapsed or absent selection does
    // not, so copying from the input still works. ---

    #[test]
    fn active_buffer_selection_gates_copy_priority() {
        let ctx = eframe::egui::Context::default();

        // No selection stored: input keeps the clipboard.
        assert!(!VellumGuiApp::active_buffer_selection_present(&ctx));

        // A collapsed selection (anchor == head) is just a caret, not a
        // highlight -- the input still owns Copy.
        VellumGuiApp::store_buffer_selection(
            &ctx,
            Some(GuiBufferSelection {
                scroll_id: "main".into(),
                anchor: (10, 3),
                head: (10, 3),
                dragging: false,
            }),
        );
        assert!(!VellumGuiApp::active_buffer_selection_present(&ctx));

        // A real, non-empty selection takes priority.
        VellumGuiApp::store_buffer_selection(
            &ctx,
            Some(GuiBufferSelection {
                scroll_id: "main".into(),
                anchor: (10, 3),
                head: (11, 0),
                dragging: false,
            }),
        );
        assert!(VellumGuiApp::active_buffer_selection_present(&ctx));

        // Clearing the selection returns the clipboard to the input.
        VellumGuiApp::store_buffer_selection(&ctx, None);
        assert!(!VellumGuiApp::active_buffer_selection_present(&ctx));
    }
}




