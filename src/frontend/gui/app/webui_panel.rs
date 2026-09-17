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
use crate::data::webui::{
    WebUiClientMessage, WebUiLayer, WebUiLayerColor, WebUiNode, WebUiPanelContent,
};

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
            egui::Id::new(("webui", page, node.cid.as_deref().unwrap_or(""), suffix))
        };

        match node.t.as_str() {
            "page" => Self::render_webui_nodes(ui, page, node.children()),
            "text" => {
                // emphasis is how the contract spells what the retired
                // `header` node used to be: strong text is a heading.
                let body = node.text.as_deref().unwrap_or("");
                match node.emphasis.as_deref() {
                    Some("strong") => ui.label(RichText::new(body).heading()),
                    Some("subtle") => ui.label(RichText::new(body).weak()),
                    _ => ui.label(body),
                };
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
            "textarea" => {
                // Multi-line text field (webui-new-nodes-for-vellum.md).
                // Same edit-state machine as text_input: keep a local
                // buffer, never clobber focused in-progress text, commit
                // the full string on blur only (Enter inserts a newline,
                // which is TextEdit::multiline's default).
                let server_value = node.value_str().unwrap_or("");
                let buf_id = scratch_id("ta_buf");
                let seen_id = scratch_id("ta_seen");
                let focus_id = scratch_id("ta_focused");

                let was_focused: bool = ui.data(|d| d.get_temp(focus_id)).unwrap_or(false);
                let last_seen: String = ui.data(|d| d.get_temp(seen_id)).unwrap_or_default();
                let mut buffer: String = ui
                    .data(|d| d.get_temp(buf_id))
                    .unwrap_or_else(|| server_value.to_string());
                if server_value != last_seen && !was_focused {
                    buffer = server_value.to_string();
                }

                if let Some(label) = node.label.as_deref() {
                    if !label.is_empty() {
                        ui.label(label);
                    }
                }
                let rows = node.rows_hint.unwrap_or(4).clamp(2, 40) as usize;
                let mut edit = egui::TextEdit::multiline(&mut buffer)
                    .id_salt(scratch_id("ta_edit"))
                    .desired_width(f32::INFINITY)
                    .desired_rows(rows);
                if let Some(hint) = node.placeholder.as_deref() {
                    edit = edit.hint_text(hint);
                }
                let response = ui.add(edit);

                if response.lost_focus() && buffer != server_value {
                    queue_event(
                        ui.ctx(),
                        page,
                        node.cid.as_deref(),
                        serde_json::Value::String(buffer.clone()),
                    );
                }
                // has_focus() re-enters the ctx lock: read BEFORE data_mut.
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
                                if ui.selectable_label(*option == current, option).clicked() {
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
                            let mut drag = egui::DragValue::new(&mut value).speed(if step > 0.0 {
                                step
                            } else {
                                1.0
                            });
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
            // A menu is a definition, not a place on the page: other
            // components point at it by key via `context_menu`, and the
            // viewer opens it with a secondary click. `bar: true` is the
            // exception — that one is a visible strip.
            "menu" => {
                if node.bar.unwrap_or(false) {
                    ui.horizontal_wrapped(|ui| {
                        Self::render_webui_nodes(ui, page, node.children());
                    });
                }
                // Non-bar menus render only when opened from their owner.
            }
            "menu_item" => {
                let label = node.label.as_deref().unwrap_or("");
                match node.kind.as_deref() {
                    Some("separator") => {
                        ui.separator();
                    }
                    Some("check") => {
                        let mut on = node.active.unwrap_or(false);
                        if ui.checkbox(&mut on, label).clicked() {
                            queue_event(
                                ui.ctx(),
                                page,
                                node.cid.as_deref(),
                                serde_json::Value::Bool(on),
                            );
                        }
                    }
                    Some("radio") => {
                        if ui.radio(node.active.unwrap_or(false), label).clicked() {
                            queue_event(
                                ui.ctx(),
                                page,
                                node.cid.as_deref(),
                                serde_json::Value::Bool(true),
                            );
                        }
                    }
                    _ => {
                        if ui.button(label).clicked() {
                            queue_event(ui.ctx(), page, node.cid.as_deref(), serde_json::Value::Null);
                        }
                        // A submenu carries children; render them inline
                        // rather than nesting popups, which egui would need
                        // a persistent open-state to do properly.
                        if !node.children().is_empty() {
                            ui.indent(scratch_id("submenu"), |ui| {
                                Self::render_webui_nodes(ui, page, node.children());
                            });
                        }
                    }
                }
            }
            // A prompt the script is BLOCKED on. Unlike every other
            // component this is not part of the page flow: it floats over
            // the app as a real modal, because a docked panel the user has
            // scrolled away from is not somewhere a blocking question can
            // sit unanswered. The panel keeps a one-line placeholder so the
            // page does not appear to have lost a component.
            "dialog" => {
                let title = node.title.as_deref().unwrap_or("Dialog");
                let buttons = node.buttons.as_deref().unwrap_or(&[]);
                if buttons.is_empty() {
                    // No way to answer: say so rather than floating a modal
                    // the user cannot dismiss.
                    ui.label(
                        RichText::new(format!("[dialog \"{title}\" has no buttons]"))
                            .weak()
                            .italics(),
                    );
                    return;
                }
                ui.label(RichText::new(format!("[dialog: {title}]")).weak().italics());

                let mut answered: Option<String> = None;
                egui::Modal::new(scratch_id("dialog")).show(ui.ctx(), |ui| {
                    ui.set_min_width(320.0);
                    ui.heading(title);
                    if let Some(body) = node.body.as_deref().filter(|b| !b.is_empty()) {
                        ui.add_space(4.0);
                        ui.label(body);
                    }
                    // Children are the contract's escape hatch for a richer
                    // prompt than prose: render them between body and
                    // buttons, where a form would go.
                    if !node.children().is_empty() {
                        ui.add_space(4.0);
                        Self::render_webui_nodes(ui, page, node.children());
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        for button in buttons {
                            let is_default = node.default_button.as_deref() == Some(&button.id);
                            let mut label = RichText::new(&button.label);
                            match button.variant.as_deref() {
                                Some("danger") => {
                                    label = label.color(egui::Color32::from_rgb(0xe5, 0x39, 0x35))
                                }
                                Some("primary") => label = label.strong(),
                                _ => {}
                            }
                            if is_default {
                                label = label.strong();
                            }
                            if ui.button(label).clicked() {
                                answered = Some(button.id.clone());
                            }
                        }
                    });
                    // Enter answers the default button, the one keyboard
                    // affordance a blocking prompt really needs.
                    if answered.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(default) = node.default_button.as_deref() {
                            answered = Some(default.to_string());
                        }
                    }
                });

                if let Some(button) = answered {
                    queue_event(
                        ui.ctx(),
                        page,
                        node.cid.as_deref(),
                        serde_json::json!({ "button": button }),
                    );
                }
            }
            // A plain grouping box. `label` titles it; `collapsible` folds
            // it away, which is an expander in all but name, so it reuses
            // that widget rather than growing a second one.
            "group" => {
                let label = node.label.as_deref().unwrap_or("");
                if node.collapsible.unwrap_or(false) {
                    egui::CollapsingHeader::new(label)
                        .id_salt(scratch_id("group"))
                        .default_open(node.open.unwrap_or(true))
                        .show(ui, |ui| {
                            Self::render_webui_nodes(ui, page, node.children());
                        });
                } else {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        if !label.is_empty() {
                            ui.label(RichText::new(label).strong());
                        }
                        Self::render_webui_nodes(ui, page, node.children());
                    });
                }
            }
            // Vertical run of children with an optional gap. The default
            // layout is already vertical, so this is spacing plus nesting.
            "stack" => {
                let gap = node.gap;
                ui.vertical(|ui| {
                    if let Some(gap) = gap {
                        ui.spacing_mut().item_spacing.y = gap.max(0.0);
                    }
                    Self::render_webui_nodes(ui, page, node.children());
                });
            }
            // Children stacked on one axis with a scrollbar. max_height 0 or
            // absent means "as tall as it wants".
            "scroll" => {
                let mut area = egui::ScrollArea::vertical().id_salt(scratch_id("scroll"));
                if let Some(max) = node.max_height.filter(|h| *h > 0.0) {
                    area = area.max_height(max);
                }
                area.show(ui, |ui| {
                    Self::render_webui_nodes(ui, page, node.children());
                });
            }
            // Children drawn at the same origin, last on top. egui has no
            // z-stack primitive, so this paints them into one cell and lets
            // later children cover earlier ones.
            "overlay" => {
                let children = node.children();
                if children.is_empty() {
                    return;
                }
                let origin = ui.cursor().min;
                let mut extent = egui::Vec2::ZERO;
                for child in children {
                    let mut area = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_min_size(origin, ui.available_size())),
                    );
                    Self::render_webui_node(&mut area, page, child);
                    extent = extent.max(area.min_rect().size());
                }
                ui.allocate_space(extent);
            }
            // Two named slots (first, second) with a divider. The contract
            // gives `position` as a fraction; egui has no splitter widget, so
            // the fraction sizes the first slot and the rest goes to second.
            "split" => {
                let children = node.children();
                let first = children.first();
                let second = children.get(1);
                let pos = node.position.unwrap_or(0.5).clamp(0.05, 0.95);
                let horizontal = node.orientation.as_deref() != Some("vertical");
                if horizontal {
                    let total = ui.available_width();
                    ui.horizontal_top(|ui| {
                        if let Some(child) = first {
                            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(
                                egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    egui::vec2(total * pos, ui.available_height()),
                                ),
                            ));
                            Self::render_webui_node(&mut left, page, child);
                            ui.allocate_space(left.min_rect().size());
                        }
                        ui.separator();
                        if let Some(child) = second {
                            Self::render_webui_node(ui, page, child);
                        }
                    });
                } else {
                    ui.vertical(|ui| {
                        if let Some(child) = first {
                            Self::render_webui_node(ui, page, child);
                        }
                        ui.separator();
                        if let Some(child) = second {
                            Self::render_webui_node(ui, page, child);
                        }
                    });
                }
            }
            // A labelled on/off switch. egui has no switch widget; a checkbox
            // carries the same state and the same change event.
            "toggle" => {
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
                let spacing = if compact {
                    2.0
                } else {
                    ui.spacing().item_spacing.x
                };
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
                            ui.allocate_ui_with_layout(egui::vec2(cell_width, 0.0), layout, |ui| {
                                ui.set_width(cell_width);
                                Self::render_webui_nodes(ui, page, cell.children());
                            });
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
                let Some(src) = node.src.as_deref() else {
                    return;
                };
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
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
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
            "composite" => {
                Self::render_webui_composite(ui, page, node, scratch_id("composite"))
            }
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
    /// `composite`: a fixed-size surface painted from an ordered draw list.
    ///
    /// Replaces the old VellumFE-only `image_map`, which could express one
    /// image plus hit boxes. The contract models the same idea as `layers`
    /// — image, bar, label and region — painted in order, so a map is art
    /// plus markers plus text plus click targets in one node.
    ///
    /// Coordinates are UNSCALED surface pixels; everything is placed at
    /// `coord * scale`, matching the browser bundle so a script callback
    /// cannot tell the two renderers apart.
    fn render_webui_composite(ui: &mut egui::Ui, page: &str, node: &WebUiNode, id: egui::Id) {
        let layers = node.layers.as_deref().unwrap_or(&[]);
        if layers.is_empty() {
            return;
        }
        let scale = node.scale.unwrap_or(1.0).max(0.01);
        // width/height are required by the contract; fall back to the
        // bounding box of what we were given rather than drawing nothing.
        let surface = egui::vec2(
            node.size.map(|s| s[0]).unwrap_or(0.0).max(0.0),
            node.size.map(|s| s[1]).unwrap_or(0.0).max(0.0),
        );
        let extent = if surface.x > 0.0 && surface.y > 0.0 {
            surface * scale
        } else {
            Self::composite_bounds(layers) * scale
        };
        if extent.x <= 0.0 || extent.y <= 0.0 {
            return;
        }

        egui::ScrollArea::both()
            .id_salt(id.with("scroll"))
            .max_height(extent.y.min(480.0))
            .auto_shrink([true, true])
            .show(ui, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(extent, egui::Sense::click());
                if !ui.is_rect_visible(rect) {
                    return;
                }
                let at = |x: f32, y: f32| rect.min + egui::vec2(x * scale, y * scale);

                for layer in layers {
                    Self::paint_composite_layer(ui, layer, scale, &at);
                }

                // Regions are hit tests, painted last so nothing covers
                // them. The topmost one under the pointer wins, matching
                // the browser, so iterate in reverse.
                if let Some(pos) = response.interact_pointer_pos().filter(|_| response.clicked()) {
                    for layer in layers.iter().rev() {
                        if let WebUiLayer::Region {
                            key,
                            x1,
                            y1,
                            x2,
                            y2,
                            activates,
                            ..
                        } = layer
                        {
                            if activates == &Some(false) {
                                continue;
                            }
                            let hit = egui::Rect::from_two_pos(at(*x1, *y1), at(*x2, *y2));
                            if hit.contains(pos) {
                                queue_event(
                                    ui.ctx(),
                                    page,
                                    node.cid.as_deref(),
                                    serde_json::json!({ "region": key }),
                                );
                                break;
                            }
                        }
                    }
                }
            });
    }

    /// Paints one layer. Regions draw nothing — they are hit tests — but a
    /// labelled one still gets a tooltip-ish outline so the surface is not
    /// silently interactive.
    fn paint_composite_layer(
        ui: &mut egui::Ui,
        layer: &WebUiLayer,
        scale: f32,
        at: &dyn Fn(f32, f32) -> egui::Pos2,
    ) {
        match layer {
            WebUiLayer::Image {
                src,
                x,
                y,
                w,
                h,
                opacity,
                tint,
            } => {
                let WebUiImageState::Ready(texture) = Self::resolve_webui_image(ui, src) else {
                    // Still loading or failed: skip this layer rather than
                    // abandoning the surface, so the rest of the map draws.
                    return;
                };
                let natural = texture.size_vec2();
                let size = egui::vec2(w.unwrap_or(natural.x), h.unwrap_or(natural.y)) * scale;
                let rect = egui::Rect::from_min_size(at(*x, *y), size);
                let mut color = tint
                    .as_ref()
                    .map(|c| Self::composite_color(ui, c))
                    .unwrap_or(egui::Color32::WHITE);
                if let Some(alpha) = opacity {
                    color = color.gamma_multiply(alpha.clamp(0.0, 1.0));
                }
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    color,
                );
            }
            WebUiLayer::Bar {
                x,
                y,
                w,
                h,
                value,
                tone,
                orientation,
            } => {
                let full = egui::Rect::from_min_size(at(*x, *y), egui::vec2(*w, *h) * scale);
                let filled = value.clamp(0.0, 1.0);
                let painter = ui.painter();
                painter.rect_filled(full, 1.0, ui.visuals().extreme_bg_color);
                let fill_rect = if orientation.as_deref() == Some("vertical") {
                    // Vertical bars fill upward from the bottom edge.
                    let height = full.height() * filled;
                    egui::Rect::from_min_max(
                        egui::pos2(full.min.x, full.max.y - height),
                        full.max,
                    )
                } else {
                    egui::Rect::from_min_size(
                        full.min,
                        egui::vec2(full.width() * filled, full.height()),
                    )
                };
                let color = tone
                    .as_ref()
                    .map(|c| Self::composite_color(ui, c))
                    .unwrap_or(ui.visuals().selection.bg_fill);
                painter.rect_filled(fill_rect, 1.0, color);
            }
            WebUiLayer::Label {
                x,
                y,
                text,
                emphasis,
                tone,
                align,
            } => {
                let anchor = match align.as_deref() {
                    Some("center") => egui::Align2::CENTER_TOP,
                    Some("end") => egui::Align2::RIGHT_TOP,
                    _ => egui::Align2::LEFT_TOP,
                };
                let style = if emphasis.as_deref() == Some("strong") {
                    egui::TextStyle::Heading
                } else {
                    egui::TextStyle::Body
                };
                let mut color = ui.visuals().text_color();
                if let Some(tone) = tone {
                    color = Self::composite_color(ui, &WebUiLayerColor::Tone(tone.clone()));
                }
                if emphasis.as_deref() == Some("subtle") {
                    color = color.gamma_multiply(0.6);
                }
                ui.painter().text(
                    at(*x, *y),
                    anchor,
                    text,
                    egui::FontId::from(style.resolve(ui.style())),
                    color,
                );
            }
            // A region paints nothing; it is a hit box resolved on click.
            WebUiLayer::Region { .. } | WebUiLayer::Unknown => {}
        }
    }

    /// Resolves a layer colour: a contract tone maps onto the theme so a
    /// `danger` bar matches the rest of the UI, and explicit channels are
    /// taken verbatim.
    fn composite_color(ui: &egui::Ui, color: &WebUiLayerColor) -> egui::Color32 {
        let tone = match color {
            WebUiLayerColor::Rgb { r, g, b, a } => {
                return egui::Color32::from_rgba_unmultiplied(*r, *g, *b, a.unwrap_or(255))
            }
            WebUiLayerColor::Tone(tone) => tone.as_str(),
            WebUiLayerColor::ToneRecord { tone } => tone.as_str(),
        };
        match tone {
            "positive" => egui::Color32::from_rgb(0x4c, 0xaf, 0x50),
            "caution" => egui::Color32::from_rgb(0xff, 0xa7, 0x26),
            "danger" => egui::Color32::from_rgb(0xe5, 0x39, 0x35),
            _ => ui.visuals().text_color(),
        }
    }

    /// Bounding box of a draw list, in unscaled surface pixels. Used only
    /// when the node omits width/height.
    fn composite_bounds(layers: &[WebUiLayer]) -> egui::Vec2 {
        let mut max = egui::Vec2::ZERO;
        let mut grow = |x: f32, y: f32| {
            max.x = max.x.max(x);
            max.y = max.y.max(y);
        };
        for layer in layers {
            match layer {
                WebUiLayer::Image { x, y, w, h, .. } => {
                    grow(x + w.unwrap_or(0.0), y + h.unwrap_or(0.0))
                }
                WebUiLayer::Bar { x, y, w, h, .. } => grow(x + w, y + h),
                WebUiLayer::Label { x, y, .. } => grow(*x, *y),
                WebUiLayer::Region { x2, y2, .. } => grow(*x2, *y2),
                WebUiLayer::Unknown => {}
            }
        }
        max
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
    const BIGSHOT: &str = include_str!("../../../../tests/data/vellum-crash-payload-bigshot.json");

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
                // egui 0.36 debug-asserts a TexturesDelta is applied before it
                // drops. This headless smoke test discards the render output
                // (no GPU to apply against), so clear the delta to satisfy it.
                let mut output = ctx.end_pass();
                output.textures_delta.clear();
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
    fn textarea_and_wound_markers_render_without_hanging() {
        // The other webui-new-nodes-for-vellum.md additions: a textarea
        // (multi-line value + rows hint) and image_map wound marker kinds.
        let raw = r#"{"type":"render","page":"nodes/demo","seq":1,
            "tree":{"t":"page","title":"Nodes","children":[
              {"t":"textarea","cid":"textarea:notes","label":"Notes",
               "value":"line one\nline two","placeholder":"...","rows_hint":5},
              {"t":"image_map","cid":"image_map:0","src":"/files/x.png","scale":1.0,
               "markers":[
                 {"id":"a","x1":0,"y1":0,"x2":8,"y2":8,"kind":"wound1"},
                 {"id":"b","x1":10,"y1":0,"x2":18,"y2":8,"kind":"wound2"},
                 {"id":"c","x1":20,"y1":0,"x2":28,"y2":8,"kind":"wound3"}]}
            ]}}"#;
        assert_renders_without_hanging(raw);
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
