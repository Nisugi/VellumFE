//! Link drag-and-drop plumbing, the tabbed-text tab strip, quickbar and
//! hotkeybar rendering, icon buttons, and gradient border painting.

use super::*;

impl VellumGuiApp {
    /// Sentinel exist_id for an item dropped onto another link;
    /// noun is "<dragged_exist_id>|<target_exist_id>".
    pub(in crate::frontend::gui::app) const LINK_DROP_SENTINEL: &'static str = "_link_drop_";

    /// egui temp-data key holding the configured item-drag modifier.
    pub(in crate::frontend::gui::app) fn drag_modifier_data_id() -> egui::Id {
        egui::Id::new("vellum_drag_modifier")
    }

    /// True while exactly the configured item-drag modifier (default Ctrl) is
    /// held. Exact matching keeps combined modifiers (e.g. Ctrl+Shift) free
    /// for keybinds and prevents AltGr (reported as Ctrl+Alt on Windows
    /// international layouts) from triggering Ctrl drags.
    pub(super) fn link_drag_modifier_down(ui: &egui::Ui) -> bool {
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
    pub(super) fn link_drag_blocks_selection(ui: &egui::Ui) -> bool {
        Self::link_drag_modifier_down(ui)
            && ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
    }

    /// Only real game entities can be dragged (not command/sentinel links).
    pub(super) fn link_is_draggable(link: &LinkData) -> bool {
        !link.exist_id.trim().is_empty() && !link.exist_id.starts_with('_')
    }

    /// Shared drag-source + drop-target handling for a link widget.
    /// Returns a drop event when another item was released onto this link.
    pub(super) fn handle_link_dnd(
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
    pub(in crate::frontend::gui::app) const TABBED_SWITCH_SENTINEL: &'static str =
        "_tabbed_switch_";

    /// Inner tab strip for tabbedtext windows. Unread tabs render bold; clicks
    /// flow through the link channel since renderers only get `&AppCore`.
    pub(super) fn render_tabbed_text_tab_strip(
        ui: &mut egui::Ui,
        window_name: &str,
        tabbed: &TabbedTextContent,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> Option<GuiLinkClick> {
        if tabbed.tabs.len() < 2 {
            return None;
        }
        // A skin skins tabs with `[controls.tab]` (+ optional `tab.active`);
        // without it, tabs fall back to egui selectable_labels.
        let tab_art = skin_art.and_then(|art| art.control_border("tab", "normal"));
        // Active-tab text must contrast with whatever fills behind it, and that
        // differs by render path:
        //   - Skinned path (tab_art present): the active tab is backed by dark
        //     `[controls.tab.active]` ART, so the label needs a LIGHT color
        //     (the accent) — dark-on-dark art was invisible (the "Story" tab).
        //   - Fallback path (selectable_label): the active tab fills with the
        //     ACCENT (selection bg), so the label needs a DARK color (window_bg)
        //     to read on it.
        // Inactive tabs use the chrome text color in both paths.
        let (tab_text, tab_active_text, tab_unread_text) = skin_art
            .and_then(|art| art.ui_palette.as_ref())
            .map(|pal| {
                // Active tab contrasts with its fill (see path note above).
                let active = if tab_art.is_some() {
                    pal.accent // light-on-dark-art
                } else {
                    pal.window_bg // dark-on-accent-fill
                };
                // Inactive tabs read as MUTED, so only the active tab stands out
                // — not the full chrome text color (which is the accent on some
                // skins, making every tab glow). Dim the palette text toward the
                // window background.
                let idle = Self::lerp_color(pal.text, pal.window_bg, 0.45);
                // Unread (non-quiet) inactive tab: the accent, so new activity
                // pops in the skin's highlight color (GUI's answer to the TUI *).
                let unread = pal.accent;
                (Some(idle), Some(active), Some(unread))
            })
            .unwrap_or((None, None, None));
        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            for (index, tab_state) in tabbed.tabs.iter().enumerate() {
                let is_active = index == tabbed.active_tab_index;
                let unread = tab_state.has_unread && !is_active;
                let mut label = RichText::new(&tab_state.definition.name);
                // Color precedence: active > unread (accent) > idle (muted).
                let color = if is_active {
                    tab_active_text
                } else if unread {
                    tab_unread_text
                } else {
                    tab_text
                };
                if let Some(color) = color {
                    label = label.color(color);
                }
                if unread {
                    label = label.strong();
                }
                let hit = if tab_art.is_some() {
                    // Skinned tab: content-sized rect, nine-slice behind a
                    // frameless label, active/normal state-keyed.
                    // Measure with the SAME text style the label renders in —
                    // a hardcoded size clipped tab names for users running a
                    // larger UI font (accessibility sizes must always fit).
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let galley = ui.painter().layout_no_wrap(
                        tab_state.definition.name.clone(),
                        font,
                        ui.visuals().text_color(),
                    );
                    let size = galley.size() + egui::vec2(16.0, 6.0);
                    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                    let state = if is_active {
                        "active"
                    } else if resp.hovered() {
                        "hover"
                    } else {
                        "normal"
                    };
                    if let Some(border) = skin_art.and_then(|art| art.control_border("tab", state))
                    {
                        crate::frontend::gui::skin::paint_nine_slice_filled(
                            ui.painter(),
                            rect,
                            border,
                        );
                    }
                    ui.put(rect, egui::Label::new(label).selectable(false));
                    resp.clicked()
                } else {
                    ui.selectable_label(is_active, label).clicked()
                };
                if hit && !is_active {
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
        // No divider line under the tab strip — the window mesh flows
        // uninterrupted into the text area (skin look). egui's separator draws
        // the theme's noninteractive stroke, which reads as a hard cyan line
        // over a dark skin.
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
                                art.icon_ref_texture(&icon.icon, icon.grayscale || button.dim)
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
                    } else if let Some(fg) = button.fg.as_deref().and_then(parse_hex_color) {
                        rich = rich.color(fg);
                    }

                    let mut widget = egui::Button::new(rich);
                    if !button.dim {
                        if let Some(bg) = button.bg.as_deref().and_then(parse_hex_color) {
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
    pub(in crate::frontend::gui::app) fn icon_edge(ui: &egui::Ui, configured: Option<u32>) -> f32 {
        match configured {
            Some(px) => px.clamp(16, 128) as f32,
            None => ui.spacing().interact_size.y.max(24.0),
        }
    }

    /// Paint one icon-faced hotbar button: allocated click rect + painter
    /// image (the codebase's sprite idiom — no egui Image widget), with
    /// optional label, solid border, dim tint, and countdown overlay.
    /// Also used by the hotbar editor's live preview.
    pub(in crate::frontend::gui::app) fn draw_icon_button(
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
            let galley = painter.layout_no_wrap(text, font, egui::Color32::WHITE);
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
    pub(super) fn gradient_t(
        dir: crate::config::GradientDir,
        pos: egui::Pos2,
        rect: egui::Rect,
    ) -> f32 {
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
    pub(super) fn paint_gradient_border(
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
            let x1 = if i + 1 == SEGMENTS {
                rect.max.x
            } else {
                x0 + step
            };
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
}
