//! Native renderer for Lich WebUI component trees.
//!
//! Walks a page's `WebUiNode` tree (see `data::webui`) and renders each
//! component as an egui widget. Interactions are queued as
//! `WebUiClientMessage::Event`s in egui context data; the app drains them
//! once per frame (`take_pending_webui_events`) and sends them over the
//! bridge socket.
//!
//! Input widgets keep local scratch state in egui temp data (keyed by
//! page + cid) so a server re-render mid-edit doesn't clobber what the
//! player is typing/dragging; the server value is (re)adopted whenever the
//! widget is not being interacted with.

use super::*;
use crate::data::webui::{WebUiClientMessage, WebUiMapMarker, WebUiNode, WebUiPanelContent};

/// State of one image in the shared cache (egui context data, keyed by src).
/// `Ready` holds the uploaded texture; the handle keeps it alive.
#[derive(Clone)]
pub(super) enum WebUiImageState {
    Loading,
    Ready(egui::TextureHandle),
    Failed(String),
}

type WebUiImageCache = std::collections::HashMap<String, WebUiImageState>;

/// Queue key for interactions produced during rendering this frame.
fn pending_events_id() -> egui::Id {
    egui::Id::new("webui_pending_events")
}

fn image_cache_id() -> egui::Id {
    egui::Id::new("webui_image_cache")
}

/// Srcs the renderer needs fetched (drained by the app once per frame).
fn pending_fetches_id() -> egui::Id {
    egui::Id::new("webui_pending_fetches")
}

/// Pages an image_map right-click asked to open as a panel.
fn pending_page_opens_id() -> egui::Id {
    egui::Id::new("webui_pending_page_opens")
}

fn queue_page_open(ctx: &egui::Context, page: &str) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<String>>(pending_page_opens_id())
            .push(page.to_string());
    });
}

/// Decodes a `data:image/...;base64,...` URI to raw encoded bytes.
fn decode_data_uri(src: &str) -> Result<Vec<u8>, String> {
    let (meta, payload) = src
        .split_once(',')
        .ok_or_else(|| "malformed data: URI".to_string())?;
    if !meta.contains("base64") {
        return Err("only base64 data: URIs are supported".to_string());
    }
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|err| err.to_string())
}

/// Display-space rect for a marker: unscaled coords * scale, offset into the
/// image rect, floored to 6px like the browser bundle.
fn marker_display_rect(marker: &WebUiMapMarker, origin: egui::Pos2, scale: f32) -> egui::Rect {
    let min = origin + egui::vec2(marker.x1 * scale, marker.y1 * scale);
    egui::Rect::from_min_size(
        min,
        egui::vec2(
            ((marker.x2 - marker.x1) * scale).max(6.0),
            ((marker.y2 - marker.y1) * scale).max(6.0),
        ),
    )
}

/// Marker visuals matching the browser bundle: "current" is a red glow
/// circle, "pin" a filled warm dot, everything else an accent-colored box.
fn paint_marker(painter: &egui::Painter, marker: &WebUiMapMarker, rect: egui::Rect, accent: Color32) {
    match marker.kind.as_deref() {
        Some("current") => {
            let red = Color32::from_rgb(229, 22, 22);
            let radius = rect.size().max_elem() / 2.0;
            painter.circle(
                rect.center(),
                radius,
                Color32::from_rgba_unmultiplied(229, 22, 22, 46),
                egui::Stroke::new(3.0, red),
            );
        }
        Some("pin") => {
            let warn = Color32::from_rgb(240, 173, 78);
            painter.circle(
                rect.center(),
                rect.size().max_elem() / 2.0,
                warn,
                egui::Stroke::new(1.0, warn),
            );
        }
        _ => {
            painter.rect_stroke(
                rect,
                3.0,
                egui::Stroke::new(2.0, accent),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn queue_event(ctx: &egui::Context, page: &str, cid: Option<&str>, value: serde_json::Value) {
    let Some(cid) = cid else { return };
    let event = WebUiClientMessage::Event {
        page: page.to_string(),
        cid: cid.to_string(),
        value,
    };
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<WebUiClientMessage>>(pending_events_id())
            .push(event);
    });
}

/// Fixed markdown span palette ({{red:...}} etc.), tuned per theme mode.
fn palette_color(name: &str, dark_mode: bool) -> Option<Color32> {
    let color = match (name, dark_mode) {
        ("red", true) => Color32::from_rgb(255, 100, 100),
        ("red", false) => Color32::from_rgb(190, 30, 30),
        ("green", true) => Color32::from_rgb(120, 220, 120),
        ("green", false) => Color32::from_rgb(20, 140, 20),
        ("blue", true) => Color32::from_rgb(120, 170, 255),
        ("blue", false) => Color32::from_rgb(30, 80, 200),
        ("yellow", true) => Color32::from_rgb(235, 220, 100),
        ("yellow", false) => Color32::from_rgb(150, 120, 0),
        ("orange", true) => Color32::from_rgb(255, 170, 90),
        ("orange", false) => Color32::from_rgb(200, 100, 0),
        ("cyan", true) => Color32::from_rgb(110, 220, 220),
        ("cyan", false) => Color32::from_rgb(0, 130, 140),
        ("magenta", true) => Color32::from_rgb(230, 130, 230),
        ("magenta", false) => Color32::from_rgb(160, 30, 160),
        ("gray", _) | ("grey", _) => Color32::from_rgb(140, 140, 140),
        _ => return None,
    };
    Some(color)
}

/// One styled run of inline-markdown text.
#[derive(Default, Clone)]
struct InlineSpan {
    text: String,
    color: Option<String>,
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
}

/// Parses the WebUI-safe inline markdown subset: `{{color:text}}` spans,
/// `**bold**`, `*italic*`, `` `code` ``, and bare http(s) URLs. Unknown or
/// unterminated markers render as plain text (matching the browser bundle's
/// forgiving behavior closely enough for panel content).
fn parse_inline_markdown(input: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut rest = input;

    while !rest.is_empty() {
        if let Some(start) = rest.find("{{") {
            if let Some(rel_end) = rest[start..].find("}}") {
                let inner = &rest[start + 2..start + rel_end];
                if let Some((color, text)) = inner.split_once(':') {
                    if !rest[..start].is_empty() {
                        parse_basic_runs(&rest[..start], None, &mut spans);
                    }
                    parse_basic_runs(text, Some(color.to_string()), &mut spans);
                    rest = &rest[start + rel_end + 2..];
                    continue;
                }
            }
        }
        parse_basic_runs(rest, None, &mut spans);
        break;
    }
    spans
}

/// Handles **bold** / *italic* / `code` / URLs within one color context.
fn parse_basic_runs(input: &str, color: Option<String>, out: &mut Vec<InlineSpan>) {
    let mut rest = input;
    let mut plain = String::new();

    let flush = |plain: &mut String, out: &mut Vec<InlineSpan>, color: &Option<String>| {
        if !plain.is_empty() {
            emit_with_links(&std::mem::take(plain), color.clone(), false, false, out);
        }
    };

    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("**") {
            if let Some(end) = stripped.find("**") {
                flush(&mut plain, out, &color);
                out.push(InlineSpan {
                    text: stripped[..end].to_string(),
                    color: color.clone(),
                    bold: true,
                    ..Default::default()
                });
                rest = &stripped[end + 2..];
                continue;
            }
        }
        if let Some(stripped) = rest.strip_prefix('*') {
            if let Some(end) = stripped.find('*') {
                flush(&mut plain, out, &color);
                out.push(InlineSpan {
                    text: stripped[..end].to_string(),
                    color: color.clone(),
                    italic: true,
                    ..Default::default()
                });
                rest = &stripped[end + 1..];
                continue;
            }
        }
        if let Some(stripped) = rest.strip_prefix('`') {
            if let Some(end) = stripped.find('`') {
                flush(&mut plain, out, &color);
                out.push(InlineSpan {
                    text: stripped[..end].to_string(),
                    color: color.clone(),
                    code: true,
                    ..Default::default()
                });
                rest = &stripped[end + 1..];
                continue;
            }
        }
        let mut chars = rest.char_indices();
        let (_, ch) = chars.next().expect("rest is non-empty");
        let next = chars.next().map(|(i, _)| i).unwrap_or(rest.len());
        plain.push(ch);
        rest = &rest[next..];
    }
    flush(&mut plain, out, &color);
}

/// Splits plain text so bare http(s) URLs become link spans.
fn emit_with_links(
    text: &str,
    color: Option<String>,
    bold: bool,
    italic: bool,
    out: &mut Vec<InlineSpan>,
) {
    let mut rest = text;
    while let Some(pos) = rest.find("http://").or_else(|| rest.find("https://")) {
        // Pick the earlier of the two protocols when both appear.
        let pos = match (rest.find("http://"), rest.find("https://")) {
            (Some(a), Some(b)) => a.min(b),
            _ => pos,
        };
        if pos > 0 {
            out.push(InlineSpan {
                text: rest[..pos].to_string(),
                color: color.clone(),
                bold,
                italic,
                ..Default::default()
            });
        }
        let url_end = rest[pos..]
            .find(|c: char| c.is_whitespace())
            .map(|i| pos + i)
            .unwrap_or(rest.len());
        out.push(InlineSpan {
            text: rest[pos..url_end].to_string(),
            link: true,
            ..Default::default()
        });
        rest = &rest[url_end..];
    }
    if !rest.is_empty() {
        out.push(InlineSpan {
            text: rest.to_string(),
            color,
            bold,
            italic,
            ..Default::default()
        });
    }
}

fn render_markdown_line(ui: &mut egui::Ui, text: &str) {
    let dark = ui.visuals().dark_mode;
    let spans = parse_inline_markdown(text);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for span in spans {
            if span.link {
                ui.hyperlink_to(span.text.clone(), span.text.clone());
                continue;
            }
            let mut rich = RichText::new(&span.text);
            if let Some(color) = span.color.as_deref().and_then(|c| palette_color(c, dark)) {
                rich = rich.color(color);
            }
            if span.bold {
                rich = rich.strong();
            }
            if span.italic {
                rich = rich.italics();
            }
            if span.code {
                rich = rich.code();
            }
            ui.label(rich);
        }
    });
}

impl VellumGuiApp {
    /// Drains interactions queued by WebUI panels during this frame.
    pub(super) fn take_pending_webui_events(ctx: &egui::Context) -> Vec<WebUiClientMessage> {
        ctx.data_mut(|d| {
            let events = d
                .get_temp::<Vec<WebUiClientMessage>>(pending_events_id())
                .unwrap_or_default();
            d.remove::<Vec<WebUiClientMessage>>(pending_events_id());
            events
        })
    }

    /// Drains image srcs the renderer asked to have fetched this frame.
    pub(super) fn take_pending_webui_fetches(ctx: &egui::Context) -> Vec<String> {
        ctx.data_mut(|d| {
            let srcs = d
                .get_temp::<Vec<String>>(pending_fetches_id())
                .unwrap_or_default();
            d.remove::<Vec<String>>(pending_fetches_id());
            srcs
        })
    }

    /// Drains pages that image_map right-clicks asked to open as panels.
    pub(super) fn take_pending_webui_page_opens(ctx: &egui::Context) -> Vec<String> {
        ctx.data_mut(|d| {
            let pages = d
                .get_temp::<Vec<String>>(pending_page_opens_id())
                .unwrap_or_default();
            d.remove::<Vec<String>>(pending_page_opens_id());
            pages
        })
    }

    pub(super) fn set_webui_image(ctx: &egui::Context, src: String, state: WebUiImageState) {
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<WebUiImageCache>(image_cache_id())
                .insert(src, state);
        });
    }

    /// Drops non-Ready entries so the renderer re-requests them. Called when
    /// a fresh bridge connection makes retries worthwhile: Failed entries
    /// get a second chance, and Loading entries are orphans by definition
    /// (their fetch task died with the old bridge or never dispatched).
    pub(super) fn clear_stale_webui_images(ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<WebUiImageCache>(image_cache_id())
                .retain(|_, state| matches!(state, WebUiImageState::Ready(_)));
        });
    }

    /// Decodes encoded image bytes and uploads them as an egui texture.
    pub(super) fn decode_webui_image(
        ctx: &egui::Context,
        src: &str,
        bytes: &[u8],
    ) -> WebUiImageState {
        match image::load_from_memory(bytes) {
            Ok(dynamic) => {
                let rgba = dynamic.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                WebUiImageState::Ready(ctx.load_texture(
                    format!("webui:{}", src),
                    color,
                    egui::TextureOptions::LINEAR,
                ))
            }
            Err(err) => WebUiImageState::Failed(format!("decode failed: {}", err)),
        }
    }

    /// Cache lookup; on a miss, data: URIs decode inline and everything else
    /// is queued for the app to fetch over the bridge.
    fn resolve_webui_image(ui: &egui::Ui, src: &str) -> WebUiImageState {
        let cached = ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<WebUiImageCache>(image_cache_id())
                .get(src)
                .cloned()
        });
        if let Some(state) = cached {
            return state;
        }
        let state = if src.starts_with("data:") {
            match decode_data_uri(src) {
                Ok(bytes) => Self::decode_webui_image(ui.ctx(), src, &bytes),
                Err(err) => WebUiImageState::Failed(err),
            }
        } else {
            ui.ctx().data_mut(|d| {
                d.get_temp_mut_or_default::<Vec<String>>(pending_fetches_id())
                    .push(src.to_string());
            });
            WebUiImageState::Loading
        };
        Self::set_webui_image(ui.ctx(), src.to_string(), state.clone());
        state
    }

    /// Renders one WebUI panel window. Interactions are queued in egui
    /// context data and drained by the app after the frame renders.
    pub(super) fn render_webui_content(ui: &mut egui::Ui, content: &WebUiPanelContent) {
        if let Some(reason) = &content.ended {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("This page has ended").strong());
                ui.label(RichText::new(reason.as_str()).weak());
                ui.label(
                    RichText::new("It will resume automatically if the script restarts.").weak(),
                );
            });
            return;
        }

        let Some(tree) = &content.tree else {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                if content.connected {
                    ui.label(RichText::new(format!("Waiting for {}...", content.page_id)).weak());
                } else {
                    ui.label(
                        RichText::new(format!(
                            "Not connected to Lich WebUI ({}). Run .webui to connect.",
                            content.page_id
                        ))
                        .weak(),
                    );
                }
            });
            return;
        };

        egui::ScrollArea::vertical()
            .id_salt(("webui_panel", &content.page_id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !content.connected {
                    ui.label(RichText::new("(bridge disconnected - reconnecting...)").weak());
                }
                Self::render_webui_nodes(ui, &content.page_id, tree.children());
            });
    }

    fn render_webui_nodes(ui: &mut egui::Ui, page: &str, nodes: &[WebUiNode]) {
        for node in nodes {
            Self::render_webui_node(ui, page, node);
        }
    }

    fn render_webui_node(ui: &mut egui::Ui, page: &str, node: &WebUiNode) {
        let scratch_id = |suffix: &str| {
            egui::Id::new((
                "webui",
                page,
                node.cid.as_deref().unwrap_or(""),
                suffix,
            ))
        };

        match node.t.as_str() {
            "page" => Self::render_webui_nodes(ui, page, node.children()),
            "header" => {
                ui.label(RichText::new(node.text.as_deref().unwrap_or("")).heading());
            }
            "text" => {
                ui.label(node.text.as_deref().unwrap_or(""));
            }
            "markdown" => {
                render_markdown_line(ui, node.text.as_deref().unwrap_or(""));
            }
            "divider" => {
                ui.separator();
            }
            "button" => {
                let label = node.label.as_deref().unwrap_or("");
                let danger = node.variant.as_deref() == Some("danger");
                let disabled = node.disabled.unwrap_or(false);

                // Two-step confirm: first click arms for a few seconds and
                // swaps the label; a second click while armed fires.
                let armed_id = scratch_id("armed_until");
                let now = ui.input(|i| i.time);
                let armed_until: f64 = ui.data(|d| d.get_temp(armed_id)).unwrap_or(0.0);
                let armed = node.confirm.is_some() && now < armed_until;

                let shown = if armed {
                    node.confirm.as_deref().unwrap_or(label)
                } else {
                    label
                };
                let mut rich = RichText::new(shown);
                if danger || armed {
                    rich = rich.color(if ui.visuals().dark_mode {
                        Color32::from_rgb(255, 110, 110)
                    } else {
                        Color32::from_rgb(180, 20, 20)
                    });
                }
                let response = ui.add_enabled(!disabled, egui::Button::new(rich));
                if response.clicked() {
                    if node.confirm.is_some() && !armed {
                        ui.data_mut(|d| d.insert_temp(armed_id, now + 3.0));
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs(4));
                    } else {
                        ui.data_mut(|d| d.insert_temp(armed_id, 0.0f64));
                        queue_event(ui.ctx(), page, node.cid.as_deref(), serde_json::Value::Null);
                    }
                }
            }
            "text_input" | "password_input" => {
                let is_password = node.t == "password_input";
                let server_value = node.value_str().unwrap_or("");
                let buf_id = scratch_id("buf");
                let seen_id = scratch_id("seen");
                let focus_id = scratch_id("focused");

                let was_focused: bool = ui.data(|d| d.get_temp(focus_id)).unwrap_or(false);
                let last_seen: String = ui.data(|d| d.get_temp(seen_id)).unwrap_or_default();
                let mut buffer: String = ui
                    .data(|d| d.get_temp(buf_id))
                    .unwrap_or_else(|| server_value.to_string());
                // Adopt a changed server value unless the player is mid-edit.
                if server_value != last_seen && !was_focused {
                    buffer = server_value.to_string();
                }

                let response = ui
                    .horizontal(|ui| {
                        if let Some(label) = node.label.as_deref() {
                            if !label.is_empty() {
                                ui.label(label);
                            }
                        }
                        let mut edit = egui::TextEdit::singleline(&mut buffer)
                            .id_salt(scratch_id("edit"))
                            .desired_width(f32::INFINITY);
                        if is_password {
                            edit = edit.password(true);
                        }
                        if let Some(hint) = node.placeholder.as_deref() {
                            edit = edit.hint_text(hint);
                        }
                        ui.add(edit)
                    })
                    .inner;

                let submitted = response.lost_focus()
                    && (ui.input(|i| i.key_pressed(egui::Key::Enter)) || buffer != server_value);
                if submitted && (is_password || buffer != server_value) {
                    queue_event(
                        ui.ctx(),
                        page,
                        node.cid.as_deref(),
                        serde_json::Value::String(buffer.clone()),
                    );
                }
                // has_focus() re-enters the egui context; it must be read
                // BEFORE data_mut takes the context write lock (non-reentrant
                // -> permanent deadlock, the "Not Responding" setup pages).
                let focused_now = response.has_focus();
                ui.data_mut(|d| {
                    d.insert_temp(buf_id, buffer);
                    d.insert_temp(seen_id, server_value.to_string());
                    d.insert_temp(focus_id, focused_now);
                });
            }
            "select" => {
                let current = node.value_str().unwrap_or("").to_string();
                let options = node.options.as_deref().unwrap_or(&[]);
                ui.horizontal(|ui| {
                    if let Some(label) = node.label.as_deref() {
                        if !label.is_empty() {
                            ui.label(label);
                        }
                    }
                    egui::ComboBox::from_id_salt(scratch_id("combo"))
                        .selected_text(current.clone())
                        .show_ui(ui, |ui| {
                            for option in options {
                                if ui
                                    .selectable_label(*option == current, option)
                                    .clicked()
                                {
                                    queue_event(
                                        ui.ctx(),
                                        page,
                                        node.cid.as_deref(),
                                        serde_json::Value::String(option.clone()),
                                    );
                                }
                            }
                        });
                });
            }
            "radio" => {
                let current = node.value_str().unwrap_or("");
                if let Some(label) = node.label.as_deref() {
                    if !label.is_empty() {
                        ui.label(label);
                    }
                }
                for option in node.options.as_deref().unwrap_or(&[]) {
                    if ui.radio(*option == current, option).clicked() && *option != current {
                        queue_event(
                            ui.ctx(),
                            page,
                            node.cid.as_deref(),
                            serde_json::Value::String(option.clone()),
                        );
                    }
                }
            }
            "checkbox" => {
                let mut checked = node.checked.unwrap_or(false);
                if ui
                    .checkbox(&mut checked, node.label.as_deref().unwrap_or(""))
                    .changed()
                {
                    queue_event(
                        ui.ctx(),
                        page,
                        node.cid.as_deref(),
                        serde_json::Value::Bool(checked),
                    );
                }
            }
            "slider" | "number_input" => {
                let min = node.min.unwrap_or(0.0);
                let max = node.max.unwrap_or(100.0);
                let step = node.step.unwrap_or(1.0).max(0.0);
                let server_value = node.value_f64().unwrap_or(min);

                let buf_id = scratch_id("num");
                let active_id = scratch_id("num_active");
                let was_active: bool = ui.data(|d| d.get_temp(active_id)).unwrap_or(false);
                let mut value: f64 = if was_active {
                    ui.data(|d| d.get_temp(buf_id)).unwrap_or(server_value)
                } else {
                    server_value
                };

                let response = ui
                    .horizontal(|ui| {
                        if let Some(label) = node.label.as_deref() {
                            if !label.is_empty() {
                                ui.label(label);
                            }
                        }
                        if node.t == "slider" {
                            let mut slider = egui::Slider::new(&mut value, min..=max);
                            if step > 0.0 {
                                slider = slider.step_by(step);
                            }
                            ui.add(slider)
                        } else {
                            let mut drag = egui::DragValue::new(&mut value)
                                .speed(if step > 0.0 { step } else { 1.0 });
                            if node.min.is_some() || node.max.is_some() {
                                drag = drag.range(min..=max);
                            }
                            ui.add(drag)
                        }
                    })
                    .inner;

                let active = response.dragged() || response.has_focus();
                // Commit when interaction ends (drag release / blur) or on a
                // discrete change while not dragging (click on slider track).
                let commit = (was_active && !active && value != server_value)
                    || (response.changed() && !active && value != server_value);
                if commit {
                    queue_event(
                        ui.ctx(),
                        page,
                        node.cid.as_deref(),
                        serde_json::json!(value),
                    );
                }
                ui.data_mut(|d| {
                    d.insert_temp(buf_id, value);
                    d.insert_temp(active_id, active);
                });
            }
            "log" => {
                let height = node.max_height.unwrap_or(200.0);
                egui::ScrollArea::vertical()
                    .id_salt(scratch_id("log"))
                    .max_height(height)
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for line in node.lines.as_deref().unwrap_or(&[]) {
                            render_markdown_line(ui, line);
                        }
                    });
            }
            "progress" => {
                let value = node.value_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
                let mut bar = egui::ProgressBar::new(value);
                if let Some(label) = node.label.as_deref() {
                    bar = bar.text(label);
                }
                ui.add(bar);
            }
            "table" => Self::render_webui_table(ui, page, node, scratch_id("table")),
            "expander" => {
                egui::CollapsingHeader::new(node.label.as_deref().unwrap_or(""))
                    .id_salt(scratch_id("expander"))
                    .default_open(node.open.unwrap_or(false))
                    .show(ui, |ui| {
                        Self::render_webui_nodes(ui, page, node.children());
                    });
            }
            "columns" => {
                let columns = node.children();
                if columns.is_empty() {
                    return;
                }
                if node.compact.unwrap_or(false) {
                    ui.horizontal(|ui| {
                        for column in columns {
                            ui.vertical(|ui| {
                                Self::render_webui_nodes(ui, page, column.children());
                            });
                        }
                    });
                    return;
                }
                let weights: Vec<f32> = match &node.weights {
                    Some(w) if w.len() == columns.len() => w.clone(),
                    _ => vec![1.0; columns.len()],
                };
                let total: f32 = weights.iter().sum::<f32>().max(f32::EPSILON);
                let spacing = ui.spacing().item_spacing.x;
                let avail =
                    ui.available_width() - spacing * (columns.len().saturating_sub(1)) as f32;
                ui.horizontal_top(|ui| {
                    for (column, weight) in columns.iter().zip(&weights) {
                        let width = (avail * weight / total).max(10.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(width);
                                Self::render_webui_nodes(ui, page, column.children());
                            },
                        );
                    }
                });
            }
            "col" | "tab" | "cell" => Self::render_webui_nodes(ui, page, node.children()),
            "grid" => {
                // Aligned matrix (webui-grid-node.md): row-major `cell`
                // children, uniform column widths across every row (the
                // browser uses grid-template-columns: repeat(cols, 1fr)).
                // Empty cells are spacers; a lone unlabeled checkbox is
                // centered (row/column headers carry its meaning).
                let cells = node.children();
                if cells.is_empty() {
                    return;
                }
                let cols = (node.cols.unwrap_or(1).max(1) as usize).min(cells.len().max(1));
                let compact = node.compact.unwrap_or(false);
                let spacing = if compact { 2.0 } else { ui.spacing().item_spacing.x };
                let avail = ui.available_width() - spacing * (cols.saturating_sub(1)) as f32;
                let cell_width = (avail / cols as f32).max(10.0);
                for row in cells.chunks(cols) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = spacing;
                        for cell in row {
                            let center = matches!(cell.children(),
                                [only] if only.t == "checkbox"
                                    && only.label.as_deref().unwrap_or("").is_empty());
                            let layout = if center {
                                egui::Layout::top_down(egui::Align::Center)
                            } else {
                                egui::Layout::top_down(egui::Align::Min)
                            };
                            ui.allocate_ui_with_layout(
                                egui::vec2(cell_width, 0.0),
                                layout,
                                |ui| {
                                    ui.set_width(cell_width);
                                    Self::render_webui_nodes(ui, page, cell.children());
                                },
                            );
                        }
                    });
                }
            }
            "tabs" => {
                let tabs = node.children();
                if tabs.is_empty() {
                    return;
                }
                let active_id = scratch_id("active_tab");
                let mut active: usize = ui.data(|d| d.get_temp(active_id)).unwrap_or(0);
                active = active.min(tabs.len() - 1);
                ui.horizontal_wrapped(|ui| {
                    for (index, tab) in tabs.iter().enumerate() {
                        let label = tab.label.as_deref().unwrap_or("Tab");
                        if ui.selectable_label(index == active, label).clicked() {
                            active = index;
                        }
                    }
                });
                ui.data_mut(|d| d.insert_temp(active_id, active));
                ui.separator();
                Self::render_webui_nodes(ui, page, tabs[active].children());
            }
            "image" => {
                let Some(src) = node.src.as_deref() else { return };
                match Self::resolve_webui_image(ui, src) {
                    WebUiImageState::Ready(texture) => {
                        let natural = texture.size_vec2();
                        // Natural size, shrunk to fit the panel width.
                        let available = ui.available_width().max(20.0);
                        let display = if natural.x > available {
                            natural * (available / natural.x)
                        } else {
                            natural
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(display, egui::Sense::hover());
                        if ui.is_rect_visible(rect) {
                            ui.painter().image(
                                texture.id(),
                                rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        if let Some(alt) = node.alt.as_deref() {
                            response.on_hover_text(alt);
                        }
                    }
                    WebUiImageState::Loading => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.label(RichText::new("loading image...").weak());
                        });
                    }
                    WebUiImageState::Failed(err) => {
                        ui.label(
                            RichText::new(format!(
                                "[image {}: {}]",
                                node.alt.as_deref().unwrap_or(src),
                                err
                            ))
                            .weak()
                            .italics(),
                        );
                    }
                }
            }
            "image_map" => Self::render_webui_image_map(ui, page, node, scratch_id("immap")),
            other => {
                ui.label(
                    RichText::new(format!("[unsupported component: {}]", other))
                        .weak()
                        .italics(),
                );
            }
        }
    }

    /// Interactive image with positioned overlays (the map component).
    ///
    /// The image renders at natural size * scale inside a scrollable region.
    /// Clicks report `{x, y, shift, ctrl, right, marker}` with x/y in
    /// UNSCALED image pixels and marker = the topmost hit box's id - the
    /// same payload the browser bundle emits, so script callbacks can't
    /// tell the difference.
    fn render_webui_image_map(ui: &mut egui::Ui, page: &str, node: &WebUiNode, id: egui::Id) {
        let Some(src) = node.src.as_deref() else { return };
        let texture = match Self::resolve_webui_image(ui, src) {
            WebUiImageState::Ready(texture) => texture,
            WebUiImageState::Loading => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new("loading image...").weak());
                });
                return;
            }
            WebUiImageState::Failed(err) => {
                ui.label(RichText::new(format!("[image_map: {}]", err)).weak().italics());
                return;
            }
        };

        let scale = node.scale.unwrap_or(1.0).max(0.01);
        let natural = texture.size_vec2();
        let display = natural * scale;
        let markers = node.markers.as_deref().unwrap_or(&[]);
        let accent = ui.visuals().hyperlink_color;

        egui::ScrollArea::both()
            .id_salt(id.with("scroll"))
            .max_height(display.y.min(480.0))
            .auto_shrink([true, true])
            .show(ui, |ui| {
                let (rect, response) = ui.allocate_exact_size(display, egui::Sense::click());
                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    painter.image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );

                    let pointer = response.hover_pos();
                    let mut hovered_label: Option<&str> = None;
                    for marker in markers {
                        let marker_rect = marker_display_rect(marker, rect.min, scale);
                        paint_marker(painter, marker, marker_rect, accent);
                        if pointer.is_some_and(|p| marker_rect.contains(p)) {
                            hovered_label = marker.label.as_deref();
                        }
                    }
                    // Marker labels surface like the browser's title tooltip.
                    if let (Some(label), Some(pointer)) = (hovered_label, pointer) {
                        painter.text(
                            pointer + egui::vec2(12.0, -4.0),
                            egui::Align2::LEFT_BOTTOM,
                            label,
                            egui::FontId::proportional(12.0),
                            ui.visuals().strong_text_color(),
                        );
                    }
                }

                // Center on the scroll_to marker whenever it moves (or first
                // appears) - same target@x,y change detection as the browser.
                if let Some(target) = node.scroll_to.as_deref() {
                    if let Some(marker) = markers.iter().find(|m| m.id == target) {
                        let marker_rect = marker_display_rect(marker, rect.min, scale);
                        let center = marker_rect.center() - rect.min;
                        let signature = format!(
                            "{}@{},{}",
                            target,
                            center.x.round() as i64,
                            center.y.round() as i64
                        );
                        let seen_id = id.with("scrolled_to");
                        let seen: String =
                            ui.data(|d| d.get_temp(seen_id)).unwrap_or_default();
                        if seen != signature {
                            ui.data_mut(|d| d.insert_temp(seen_id, signature));
                            ui.scroll_to_rect(marker_rect, Some(egui::Align::Center));
                        }
                    }
                }

                let left = response.clicked();
                let right = response.secondary_clicked();
                if left || right {
                    if right && node.popup.is_some() {
                        // Right-click opens the named page as its own panel
                        // (the browser opens a supplemental window here).
                        queue_page_open(ui.ctx(), node.popup.as_deref().unwrap_or_default());
                    } else if let Some(pos) = response.interact_pointer_pos() {
                        // Topmost marker wins the hit like the browser's DOM
                        // (later boxes render on top).
                        let marker_hit = markers
                            .iter()
                            .rev()
                            .find(|m| marker_display_rect(m, rect.min, scale).contains(pos))
                            .map(|m| m.id.clone());
                        let unscaled = (pos - rect.min) / scale;
                        let modifiers = ui.input(|i| i.modifiers);
                        queue_event(
                            ui.ctx(),
                            page,
                            node.cid.as_deref(),
                            serde_json::json!({
                                "x": unscaled.x.round() as i64,
                                "y": unscaled.y.round() as i64,
                                "shift": modifiers.shift,
                                "ctrl": modifiers.ctrl || modifiers.mac_cmd,
                                "right": right,
                                "marker": marker_hit,
                            }),
                        );
                    }
                }
            });
    }

    fn render_webui_table(ui: &mut egui::Ui, page: &str, node: &WebUiNode, grid_id: egui::Id) {
        let rows = node.rows.as_deref().unwrap_or(&[]);
        let clickable = node.clickable.unwrap_or(false);
        let selected = node.selected.unwrap_or(-1);

        let render_grid = |ui: &mut egui::Ui| {
            egui::Grid::new(grid_id)
                .striped(true)
                .min_col_width(24.0)
                .show(ui, |ui| {
                    if let Some(headings) = node.headings.as_deref() {
                        for heading in headings {
                            ui.label(RichText::new(heading).strong());
                        }
                        ui.end_row();
                    }
                    for (index, row) in rows.iter().enumerate() {
                        let is_selected = selected == index as i64;
                        let mut row_clicked = false;
                        for cell in row {
                            let mut rich = RichText::new(cell);
                            if is_selected {
                                rich = rich.strong().color(ui.visuals().selection.stroke.color);
                            }
                            if clickable {
                                let response =
                                    ui.add(egui::Label::new(rich).sense(egui::Sense::click()));
                                if response.clicked() {
                                    row_clicked = true;
                                }
                                if response.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                            } else {
                                ui.label(rich);
                            }
                        }
                        ui.end_row();
                        if row_clicked {
                            // Index into the server's row order (schema: row
                            // clicks always report the unsorted index).
                            queue_event(
                                ui.ctx(),
                                page,
                                node.cid.as_deref(),
                                serde_json::json!(index),
                            );
                        }
                    }
                });
        };

        match node.max_height {
            Some(height) => {
                egui::ScrollArea::vertical()
                    .id_salt(grid_id.with("scroll"))
                    .max_height(height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| render_grid(ui));
            }
            None => render_grid(ui),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::data::webui::{WebUiPanelContent, WebUiServerMessage};

    // Render frames captured from live Lich sessions that hung the GUI
    // ("Not Responding") when opened as panels. See
    // lich5-docker/docs/vellum-tabs-crash-report.md.
    const ECLEANSE: &str =
        include_str!("../../../../tests/data/vellum-crash-payload-ecleanse.json");
    const BIGSHOT: &str =
        include_str!("../../../../tests/data/vellum-crash-payload-bigshot.json");

    fn content_from_capture(raw: &str) -> WebUiPanelContent {
        let msg: WebUiServerMessage = serde_json::from_str(raw).expect("captured payload parses");
        let WebUiServerMessage::Render { page, tree, .. } = msg else {
            panic!("capture is not a render envelope");
        };
        let mut content = WebUiPanelContent::new(page, "capture");
        content.tree = Some(tree);
        content.connected = true;
        content
    }

    /// Renders a captured page for several headless frames on a worker
    /// thread; the watchdog turns an infinite layout/parse loop into a test
    /// failure instead of a stuck test process.
    fn assert_renders_without_hanging(raw: impl Into<String>) {
        let raw = raw.into();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let content = content_from_capture(&raw);
            let ctx = egui::Context::default();
            for _ in 0..8 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420.0, 640.0),
                    )),
                    ..Default::default()
                };
                ctx.begin_pass(input);
                let mut root = egui::Ui::new(
                    ctx.clone(),
                    egui::Id::new("webui_panel_test_root"),
                    egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420.0, 640.0),
                    )),
                );
                super::VellumGuiApp::render_webui_content(&mut root, &content);
                let _ = ctx.end_pass();
            }
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("webui renderer hung on captured payload");
    }

    #[test]
    fn captured_ecleanse_setup_renders_without_hanging() {
        assert_renders_without_hanging(ECLEANSE);
    }

    #[test]
    fn captured_bigshot_setup_renders_without_hanging() {
        assert_renders_without_hanging(BIGSHOT);
    }

    #[test]
    fn grid_node_sample_renders_without_hanging() {
        // Spec sample from lich5-docker/docs/webui-grid-node.md, wrapped in
        // a minimal render envelope.
        let grid = include_str!("../../../../tests/data/webui-grid-node-sample.json");
        let raw = format!(
            r#"{{"type":"render","page":"grid/demo","seq":1,
                "tree":{{"t":"page","title":"Grid","children":[{}]}}}}"#,
            grid
        );
        assert_renders_without_hanging(raw);
    }
}
