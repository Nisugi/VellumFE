//! Board-style widgets: performance metrics, the dashboard grid, room,
//! active effects, targets, and players.

use super::*;

impl VellumGuiApp {
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
    pub(super) fn draw_perf_sparkline(ui: &mut egui::Ui, values: &[f32]) {
        if values.len() < 2 {
            return;
        }
        let height = ui.text_style_height(&egui::TextStyle::Monospace).max(8.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, height), egui::Sense::hover());
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
            let layer = Layer {
                id,
                value,
                resolved,
                visible,
            };
            // Merge into an existing stack cell of the same (non-empty) name;
            // otherwise open a new cell.
            match cells
                .iter_mut()
                .find(|c| !stack.is_empty() && c.stack.eq_ignore_ascii_case(&stack))
            {
                Some(cell) => cell.layers.push(layer),
                None => cells.push(Cell {
                    stack,
                    layers: vec![layer],
                }),
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
            let value = layer
                .value
                .max(if layer.resolved.state_matched { 1 } else { 0 });
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
                super::status_icons::paint(
                    ui.painter(),
                    rect,
                    id,
                    color,
                    ui.visuals().window_fill(),
                );
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
                    let value = first
                        .value
                        .max(if first.resolved.state_matched { 1 } else { 0 });
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
    /// Render `body`, floating any line that carries an inline image and
    /// wrapping the following lines beside it.
    ///
    /// The float lives here, in the caller, rather than in
    /// `render_styled_line`: a line cannot affect its successors' width, so
    /// the span has to be laid out as a unit. `ui.horizontal` with a nested
    /// `ui.vertical` gives the inset for free — the covered lines simply wrap
    /// to whatever width their parent `Ui` offers.
    ///
    /// Text rejoins full width after the image (real float semantics): only
    /// the lines that actually fit beside the image are inset, and a line that
    /// would straddle the image's bottom edge is excluded. egui's single
    /// `max_width` per layout job cannot express CSS's "shorten just this
    /// line's first rows", so excluding the straddler is the honest
    /// approximation.
    fn render_lines_with_floats(
        ui: &mut egui::Ui,
        body: &[StyledLine],
        visuals: &egui::Visuals,
        font_id: &egui::FontId,
        text_size: f32,
        // Index of the room-name line. The name is part of the flowing body
        // (so it wraps beside room art instead of sitting above it) but must
        // still read as a heading, so it renders a size larger.
        name_line: Option<usize>,
    ) -> Option<GuiLinkClick> {
        let mut clicked_link = None;
        let row_height = ui.ctx().fonts_mut(|f| f.row_height(font_id));
        let name_font = egui::FontId {
            size: text_size + 2.0,
            family: font_id.family.clone(),
        };
        let font_for = |idx: usize| -> &egui::FontId {
            if name_line == Some(idx) {
                &name_font
            } else {
                font_id
            }
        };
        let mut index = 0usize;

        while index < body.len() {
            let line = &body[index];
            let float = line
                .segments
                .iter()
                .find_map(|seg| seg.inline_image.as_ref())
                .and_then(|image| {
                    let natural =
                        super::custom_emoji_render::inline_image_size(ui.ctx(), &image.name)?;
                    Some((image, natural))
                });

            let Some((image, natural)) = float else {
                if let Some(link) =
                    Self::render_styled_line(ui, line, visuals, None, font_for(index), true, None)
                {
                    clicked_link = Some(link);
                }
                index += 1;
                continue;
            };

            let avail_w = ui.available_width().max(1.0);
            let avail_h = ui.available_height().max(row_height);
            let (img_w, img_h) = image.fitted_size(
                (natural.x, natural.y),
                row_height,
                avail_w,
                avail_h,
                crate::data::INLINE_IMAGE_MAX_ROWS,
            );

            // The image's own line usually carries prose too — a script
            // writes `<vellumImg/>The room stretches...` as ONE line — so its
            // remaining segments lead the wrapped text. Only the image
            // segment itself is dropped; its `[img:name]` text is a fallback
            // for frontends that cannot paint, not content to show here.
            let lead: Vec<TextSegment> = line
                .segments
                .iter()
                .filter(|seg| seg.inline_image.is_none())
                .cloned()
                .collect();
            let lead_line = (!lead.iter().all(|s| s.text.trim().is_empty())).then(|| StyledLine {
                segments: lead,
                stream: line.stream.clone(),
                timestamp: line.timestamp,
            });

            let text_w = (avail_w - img_w).max(1.0);
            let collapse = crate::data::InlineImage::should_collapse(img_w, avail_w);
            // The lead prose sits beside the image too, so it eats into the
            // height available to the following lines.
            let lead_height = lead_line
                .as_ref()
                .map(|l| Self::measure_styled_line(ui.ctx(), l, font_id, text_w))
                .unwrap_or(0.0);
            let covered_end = if collapse {
                index + 1
            } else {
                Self::float_covered_end(body, index, (img_h - lead_height).max(0.0), |line| {
                    Self::measure_styled_line(ui.ctx(), line, font_id, text_w)
                })
            };

            let covered = &body[index + 1..covered_end];
            let align = image.align;
            let name = image.name.clone();

            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let mut paint_image = |ui: &mut egui::Ui| {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(img_w, img_h),
                        egui::Sense::click_and_drag(),
                    );
                    super::custom_emoji_render::paint_inline_image(
                        ui.ctx(),
                        ui.painter(),
                        &name,
                        rect,
                    );
                    // Press-and-hold blows the image up to its full size, the
                    // way Wrayth does; releasing drops it back. Drawn in a
                    // foreground layer so it covers the text instead of being
                    // clipped by the float's own narrow slot.
                    if response.is_pointer_button_down_on() {
                        Self::paint_enlarged_image(ui, &name, rect);
                    }
                    response.on_hover_cursor(egui::CursorIcon::ZoomIn);
                };
                let mut run_text = |ui: &mut egui::Ui| {
                    ui.vertical(|ui| {
                        ui.set_width(text_w);
                        if let Some(lead) = &lead_line {
                            if let Some(link) = Self::render_styled_line(
                                ui,
                                lead,
                                visuals,
                                None,
                                font_for(index),
                                true,
                                None,
                            ) {
                                clicked_link = Some(link);
                            }
                        }
                        for (offset, covered_line) in covered.iter().enumerate() {
                            if let Some(link) = Self::render_styled_line(
                                ui,
                                covered_line,
                                visuals,
                                None,
                                font_for(index + 1 + offset),
                                true,
                                None,
                            ) {
                                clicked_link = Some(link);
                            }
                        }
                    });
                };
                match align {
                    crate::data::FloatAlign::Left => {
                        paint_image(ui);
                        run_text(ui);
                    }
                    crate::data::FloatAlign::Right => {
                        run_text(ui);
                        paint_image(ui);
                    }
                }
            });

            index = covered_end;
        }
        clicked_link
    }

    /// Draw `name` blown up over the window while its float is held down.
    ///
    /// The enlarged copy is fitted into the window with its aspect ratio
    /// preserved and never upscaled past the art's own pixel size, so holding
    /// a small image does not produce a blurry poster. It paints in a
    /// foreground layer above the text, with a dimmed backdrop, and is
    /// centred on the window rather than on the float — a right-aligned float
    /// would otherwise throw the enlargement off-screen.
    pub(super) fn paint_enlarged_image(ui: &egui::Ui, name: &str, origin: egui::Rect) {
        let ctx = ui.ctx();
        let Some(natural) = super::custom_emoji_render::inline_image_size(ctx, name) else {
            return;
        };
        // Fit inside the WHOLE application viewport, not the hosting
        // window — a small room window must not cap the preview (live
        // feedback, 2026-08-10). The paint already goes to a foreground
        // layer, so only this size/center math was window-bound. Still
        // never magnifies beyond the art's own 1:1.
        let bounds = ctx.content_rect();
        let avail = (bounds.size() * 0.9).max(egui::vec2(1.0, 1.0));
        let scale = (avail.x / natural.x)
            .min(avail.y / natural.y)
            .min(1.0)
            .max(f32::EPSILON);
        let size = natural * scale;
        // Never smaller than the float itself, or "enlarging" could shrink it.
        let size = size.max(origin.size());
        let rect = egui::Rect::from_center_size(bounds.center(), size);

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new(("inline_image_zoom", name)),
        ));
        painter.rect_filled(bounds, 0.0, egui::Color32::from_black_alpha(160));
        super::custom_emoji_render::paint_inline_image(ctx, &painter, name, rect);
    }

    /// Index one past the last line a float covers.
    ///
    /// Lines are taken greedily at the inset width while they fit within
    /// `image_height`. A line that would straddle the image's bottom edge is
    /// excluded, so it renders full width — text rejoins after the image
    /// rather than staying indented forever. A line carrying its own image
    /// also ends the span: stacked floats are out of scope, and a hard break
    /// beats overlapping pictures.
    ///
    /// `measure` returns a line's wrapped height at the inset width; it is a
    /// parameter so this decision is testable without an egui context.
    pub(super) fn float_covered_end(
        body: &[StyledLine],
        origin: usize,
        image_height: f32,
        mut measure: impl FnMut(&StyledLine) -> f32,
    ) -> usize {
        let mut end = origin + 1;
        let mut used = 0.0f32;
        while end < body.len() {
            if body[end].segments.iter().any(|s| s.inline_image.is_some()) {
                break;
            }
            let h = measure(&body[end]);
            if used + h > image_height {
                break;
            }
            used += h;
            end += 1;
        }
        end
    }

    /// Height one styled line occupies when wrapped to `wrap_width`. Used to
    /// decide how many lines fit beside a float.
    fn measure_styled_line(
        ctx: &egui::Context,
        line: &StyledLine,
        font_id: &egui::FontId,
        wrap_width: f32,
    ) -> f32 {
        let text: String = line.segments.iter().map(|s| s.text.as_str()).collect();
        if text.is_empty() {
            return ctx.fonts_mut(|f| f.row_height(font_id));
        }
        let mut job = egui::text::LayoutJob {
            wrap: egui::text::TextWrapping {
                max_width: wrap_width,
                ..Default::default()
            },
            ..Default::default()
        };
        job.append(
            &text,
            0.0,
            egui::text::TextFormat {
                font_id: font_id.clone(),
                ..Default::default()
            },
        );
        ctx.fonts_mut(|f| f.layout_job(job)).size().y
    }

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
        // The room NAME is part of the flowing body, not a header above it:
        // when the room has art, the name must wrap beside the picture along
        // with the description rather than being pushed above it. It renders
        // larger and bold via its own segment styling.
        let name_line = (!room.name.is_empty()).then_some(0usize);
        if !room.name.is_empty() {
            body.push(StyledLine {
                segments: vec![TextSegment {
                    text: room.name.clone(),
                    bold: true,
                    span_type: crate::data::SpanType::Normal,
                    ..Default::default()
                }],
                stream: "room".to_string(),
                timestamp: None,
            });
        }
        if show_desc {
            body.extend(room.description.iter().cloned());
        }
        // The art rides on the description's first line (room_sync merges it
        // there). Now that the NAME leads the body, hoist the art onto the
        // name line so the float starts at the top and the name wraps beside
        // the picture instead of sitting above it.
        if body.len() > 1 {
            let art: Vec<TextSegment> = body
                .get(1)
                .map(|line| {
                    line.segments
                        .iter()
                        .filter(|s| s.inline_image.is_some())
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if !art.is_empty() {
                if let Some(desc) = body.get_mut(1) {
                    desc.segments.retain(|s| s.inline_image.is_none());
                }
                if let Some(name_line) = body.first_mut() {
                    let mut lead = art;
                    lead.append(&mut name_line.segments);
                    name_line.segments = lead;
                }
            }
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
            let sel = widget_accent(ui.ctx(), visuals);
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
                if let Some(link) = Self::render_lines_with_floats(
                    ui, &body, visuals, font_id, text_size, name_line,
                ) {
                    clicked_link = Some(link);
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
        content_align: Option<&str>,
    ) {
        if effects_content.effects.is_empty() {
            // content_align applies ONLY to this placeholder (owner ask):
            // the effect bars themselves always fill top-down.
            let text = format!("No active {}.", effects_content.category);
            match content_align.map(crate::config::ContentAlign::from_str) {
                None => {
                    ui.label(text);
                }
                Some(align) => {
                    use crate::config::ContentAlign as CA;
                    let anchor = match align {
                        CA::TopLeft => egui::Align2::LEFT_TOP,
                        CA::Top => egui::Align2::CENTER_TOP,
                        CA::TopRight => egui::Align2::RIGHT_TOP,
                        CA::Left => egui::Align2::LEFT_CENTER,
                        CA::Center => egui::Align2::CENTER_CENTER,
                        CA::Right => egui::Align2::RIGHT_CENTER,
                        CA::BottomLeft => egui::Align2::LEFT_BOTTOM,
                        CA::Bottom => egui::Align2::CENTER_BOTTOM,
                        CA::BottomRight => egui::Align2::RIGHT_BOTTOM,
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                    ui.painter().text(
                        anchor.pos_in_rect(&rect),
                        anchor,
                        text,
                        egui::FontId::proportional(settings.text_size),
                        ui.visuals().text_color(),
                    );
                }
            }
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
                        .unwrap_or(widget_accent(ui.ctx(), visuals));
                    let preferred_text_color = effect
                        .text_color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or_else(|| visuals.text_color());

                    let corner_radius = settings.bar_corner_radius;
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, corner_radius, bg);
                    // Ticked to "now" when the countdown setting is on; the
                    // server's last snapshot otherwise. One clock sample per
                    // frame (captured in WidgetRenderSettings), so every bar
                    // in every window flips on the same instant.
                    let shown_value = match settings.effect_countdown_now {
                        Some(now_server) => effect.display_value(now_server),
                        None => effect.value,
                    };
                    let fraction = (shown_value.min(100) as f32) / 100.0;
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
                    let ticked_time = settings
                        .effect_countdown_now
                        .map(|now_server| effect.display_time(now_server));
                    let time = ticked_time.as_deref().unwrap_or(&effect.time).trim();
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
                    let name_galley =
                        painter.layout_no_wrap(effect.text.clone(), font, Color32::PLACEHOLDER);
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

        // Keep the countdown ticking: request one repaint at the next
        // whole-second boundary while any timed effect is visible. egui
        // coalesces every pending request down to the earliest deadline, so
        // all effect windows share a single repaint per second and every bar
        // flips its digit on the same frame; combat RT's own repaints subsume
        // this entirely. Boards of Indefinite effects schedule nothing.
        if settings.effect_countdown_now.is_some()
            && effects_content.effects.iter().any(|e| e.ticks())
        {
            let ms_into_second = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| u64::from(d.subsec_millis()))
                .unwrap_or(0);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(
                    (1000 - ms_into_second).max(50),
                ));
        }
    }

    // Visibility: tested from app/tests.rs.
    pub(in crate::frontend::gui::app) fn format_target_line(
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

    // Visibility: tested from app/tests.rs.
    pub(in crate::frontend::gui::app) fn format_player_line(
        player: &crate::core::state::Player,
        target_cfg: &TargetListConfig,
    ) -> String {
        let mut statuses = Vec::new();
        // Dead marker leads (reads "Name [ded] [prn]"), via the same abbrev map.
        if player.dead {
            statuses.push(format!(
                "[{}]",
                Self::status_abbreviation("dead", target_cfg)
            ));
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
                    } else if let Some(color) = app_core
                        .config
                        .colors
                        .presets
                        .get("monsterbold")
                        .and_then(|preset| preset.fg.as_deref())
                        .and_then(parse_hex_color)
                    {
                        // Creatures are monsterbold text everywhere else in
                        // the client; the list wears the same preset.
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

    pub(super) fn render_players_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
    ) -> Option<GuiLinkClick> {
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

    /// Missing-spells watchlist: watched spells (`.spellwatch`) that are
    /// NOT currently active in ActiveSpells/Buffs. The list comes from
    /// `core::missing_spells`; empty states explain themselves.
    pub(super) fn render_missing_spells_content(app_core: &AppCore, ui: &mut egui::Ui) {
        let watched = &app_core.game_state.character.watched_spells;
        if watched.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(".spellwatch add <n> to watch").weak());
            });
            return;
        }
        let missing = crate::core::missing_spells::missing(&app_core.game_state);
        if missing.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("All spells up")
                        .color(egui::Color32::from_rgb(0x5f, 0x87, 0x5f)),
                );
            });
            return;
        }
        let max_height = ui.available_height().max(1.0);
        egui::ScrollArea::vertical()
            .id_salt("missing_spells_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for spell in &missing {
                    ui.label(
                        RichText::new(format!("{} {}", spell.number, spell.name))
                            .color(egui::Color32::from_rgb(0xd7, 0x87, 0x00)),
                    );
                }
            });
    }
}
