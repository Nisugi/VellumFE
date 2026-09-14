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

    /// Sentinel exist_id for a game link dropped onto a hotbar; noun is
    /// "<bar_name>|<exist_id>|<noun>|<link text>".
    pub(in crate::frontend::gui::app) const HOTBAR_DROP_SENTINEL: &'static str = "_hotbar_drop_";

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
        // A `Text` window being dragged, or a tab dragged from ANOTHER
        // tabbed window, turns this strip into a drop band, single-tab
        // strips included (spec §7); otherwise a single tab has no strip.
        let join_source = super::super::tab_join::join_drag_source(ui.ctx());
        let foreign_tab = egui::DragAndDrop::payload::<super::super::tab_drag::TabDragPayload>(
            ui.ctx(),
        )
        .filter(|payload| payload.window != window_name);
        let is_drop_band = join_source.as_deref().is_some_and(|source| source != window_name)
            || foreign_tab.is_some();
        if tabbed.tabs.len() < 2 && !is_drop_band {
            Self::render_single_tab_grip(ui, window_name, tabbed);
            return None;
        }
        // A skin skins tabs with `[controls.tab]` (+ optional `tab.active`);
        // without it, tabs fall back to egui selectable_labels.
        let tab_art = skin_art.and_then(|art| art.control_border("tab", "normal"));
        // Unread tabs read in the theme's accent (the hyperlink colour is the
        // one accent every theme defines) with a count badge; the rest keep
        // the default text colours.
        let unread_color = ui.visuals().hyperlink_color;
        let mut clicked = None;
        let mut tab_rects: Vec<(crate::data::widget::TabId, egui::Rect)> = Vec::new();

        // One row that scrolls sideways (scrollbar hidden) instead of
        // wrapping into the content area. When the row is wider than the
        // window a chevron at the right lists every tab. The chevron's
        // room is reserved from the previous frame's measurement.
        let overflow_id = ui.id().with(("tab_strip_overflow", window_name));
        let overflowed_last_frame: bool = ui
            .ctx()
            .data(|data| data.get_temp(overflow_id))
            .unwrap_or(false);
        let chevron_width = 22.0;
        let full_width = ui.available_width();
        let row_width = if overflowed_last_frame {
            (full_width - chevron_width - 4.0).max(40.0)
        } else {
            full_width
        };
        let mut active_rect: Option<egui::Rect> = None;
        let mut active_visible = true;
        let strip = ui.horizontal(|ui| {
            let scroll = egui::ScrollArea::horizontal()
                .id_salt(("tab_strip_scroll", window_name))
                .max_width(row_width)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, tab_state) in tabbed.tabs.iter().enumerate() {
                            let is_active = index == tabbed.active_tab_index;
                            let unread = tab_state.has_unread && !is_active;
                            let mut text = tab_state.definition.name.clone();
                            if unread && tab_state.unread_count > 0 {
                                let shown = if tab_state.unread_count > 99 {
                                    "99+".to_string()
                                } else {
                                    tab_state.unread_count.to_string()
                                };
                                text = format!("{text} ({shown})");
                            }
                            let mut label = RichText::new(&text);
                            if unread {
                                label = label.color(unread_color).strong();
                            }
                            let hit = if tab_art.is_some() {
                                // Skinned tab: content-sized rect, nine-slice
                                // behind a frameless label, state-keyed.
                                // Measured in the label's own text style so
                                // larger accessibility fonts still fit.
                                let font = egui::TextStyle::Body.resolve(ui.style());
                                let galley = ui.painter().layout_no_wrap(
                                    text.clone(),
                                    font,
                                    ui.visuals().text_color(),
                                );
                                let size = galley.size() + egui::vec2(16.0, 6.0);
                                let (rect, resp) =
                                    ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                                let state = if is_active {
                                    "active"
                                } else if resp.hovered() {
                                    "hover"
                                } else {
                                    "normal"
                                };
                                if let Some(border) =
                                    skin_art.and_then(|art| art.control_border("tab", state))
                                {
                                    crate::frontend::gui::skin::paint_nine_slice_filled(
                                        ui.painter(),
                                        rect,
                                        border,
                                    );
                                }
                                ui.put(rect, egui::Label::new(label).selectable(false));
                                tab_rects.push((tab_state.definition.id, rect));
                                if is_active {
                                    active_rect = Some(rect);
                                    active_visible = ui.clip_rect().contains_rect(rect);
                                }
                                Self::arm_tab_drag(&resp, window_name, tab_state);
                                resp.clicked()
                            } else {
                                let resp = ui.selectable_label(is_active, label);
                                tab_rects.push((tab_state.definition.id, resp.rect));
                                if is_active {
                                    active_rect = Some(resp.rect);
                                    active_visible = ui.clip_rect().contains_rect(resp.rect);
                                }
                                // Re-register with a drag sense so a press that
                                // travels becomes a tear-out; a click selects.
                                Self::arm_tab_drag(
                                    &resp.interact(egui::Sense::drag()),
                                    window_name,
                                    tab_state,
                                );
                                resp.clicked()
                            };
                            if hit && !is_active {
                                clicked = Some(Self::tab_switch_click(window_name, index));
                            }
                        }
                    });
                });
            let overflowed = scroll.content_size.x > scroll.inner_rect.width() + 0.5;
            ui.ctx()
                .data_mut(|data| data.insert_temp(overflow_id, overflowed));
            // The active tab scrolled out of the row (a keybind cycled to
            // it, or a new tab was appended): bring it back into view.
            if let (Some(rect), false) = (active_rect, active_visible) {
                // Content-space x extent of the active tab.
                let mut state = scroll.state;
                let min_x = rect.min.x - scroll.inner_rect.min.x + state.offset.x;
                let max_x = rect.max.x - scroll.inner_rect.min.x + state.offset.x;
                let visible_w = scroll.inner_rect.width();
                if min_x < state.offset.x {
                    state.offset.x = min_x;
                } else if max_x > state.offset.x + visible_w {
                    state.offset.x = max_x - visible_w;
                }
                state.store(ui.ctx(), scroll.id);
                ui.ctx().request_repaint();
            }
            if overflowed {
                let unread_total = tabbed
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(i, t)| *i != tabbed.active_tab_index && t.has_unread)
                    .count();
                let mut chevron = RichText::new("»");
                if unread_total > 0 {
                    chevron = chevron.color(unread_color).strong();
                }
                ui.menu_button(chevron, |ui| {
                    ui.set_min_width(140.0);
                    for (index, tab_state) in tabbed.tabs.iter().enumerate() {
                        let is_active = index == tabbed.active_tab_index;
                        let unread = tab_state.has_unread && !is_active;
                        let mut text = tab_state.definition.name.clone();
                        if unread && tab_state.unread_count > 0 {
                            text = format!("{text} ({})", tab_state.unread_count);
                        }
                        let mut label = RichText::new(text);
                        if unread {
                            label = label.color(unread_color).strong();
                        }
                        if ui.selectable_label(is_active, label).clicked() {
                            if !is_active {
                                clicked = Some(Self::tab_switch_click(window_name, index));
                            }
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("All tabs");
            }
        });
        // Widen the recorded strip to the window's full width so the band
        // (and the dwell hit test) covers the whole row, not just the labels.
        let mut strip_rect = strip.response.rect;
        strip_rect.min.x = strip_rect.min.x.min(ui.max_rect().min.x);
        strip_rect.max.x = strip_rect.max.x.max(ui.max_rect().max.x);
        super::super::tab_drag::record_tab_strip_rect(ui.ctx(), window_name, strip_rect);
        super::super::tab_drag::record_tab_rects(ui.ctx(), window_name, tab_rects);
        if is_drop_band {
            // A foreign tab arms the instant it is over the row; a window
            // join arms after its dwell.
            let armed = if foreign_tab.is_some() {
                ui.rect_contains_pointer(strip_rect)
            } else {
                super::super::tab_join::join_dwell_target(ui.ctx())
                    .is_some_and(|(target, armed)| target == window_name && armed)
            };
            let hovering = ui.rect_contains_pointer(strip_rect);
            let (fill, stroke) = if armed {
                (
                    Color32::from_rgba_unmultiplied(70, 180, 90, 70),
                    egui::Stroke::new(2.0, Color32::from_rgb(90, 200, 110)),
                )
            } else if hovering {
                (
                    Color32::from_rgba_unmultiplied(70, 130, 220, 60),
                    egui::Stroke::new(1.5, Color32::from_rgb(90, 150, 240)),
                )
            } else {
                (
                    Color32::from_rgba_unmultiplied(70, 130, 220, 28),
                    egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 150, 240, 120)),
                )
            };
            let band = strip_rect.expand2(egui::vec2(0.0, 2.0));
            ui.painter()
                .rect(band, 3.0, fill, stroke, egui::StrokeKind::Inside);
        }
        // No divider line under the tab strip — the window mesh flows
        // uninterrupted into the text area (skin look).
        clicked
    }

    /// The click that switches `window_name` to tab `index`, routed through
    /// the link path like every other in-window click.
    fn tab_switch_click(window_name: &str, index: usize) -> GuiLinkClick {
        GuiLinkClick {
            link_data: LinkData {
                exist_id: Self::TABBED_SWITCH_SENTINEL.to_string(),
                noun: format!("{}|{}", window_name, index),
                text: String::new(),
                coord: None,
            },
            click_pos: (0, 0),
        }
    }

    /// A single-tab window has no strip, so its only tab would be
    /// unreachable by drag. While the pointer is over the window, show a
    /// small grip in the top-right corner that acts as that tab (spec 6,
    /// last-tab reachability; the context menu remains the guaranteed way).
    fn render_single_tab_grip(ui: &mut egui::Ui, window_name: &str, tabbed: &TabbedTextContent) {
        let Some(tab_state) = tabbed.tabs.first() else {
            return;
        };
        let dragging_this =
            egui::DragAndDrop::payload::<super::super::tab_drag::TabDragPayload>(ui.ctx())
                .is_some_and(|payload| payload.window == window_name);
        if !ui.rect_contains_pointer(ui.max_rect()) && !dragging_this {
            return;
        }
        let size = egui::vec2(22.0, 14.0);
        let rect = egui::Rect::from_min_size(
            ui.max_rect().right_top() - egui::vec2(size.x + 2.0, -2.0),
            size,
        );
        let id = ui.id().with(("single_tab_grip", window_name));
        let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
        let visuals = ui.style().interact(&resp);
        ui.painter().rect(
            rect,
            3.0,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        // Two rows of three dots: the universal "grab here" glyph.
        for row in 0..2 {
            for col in 0..3 {
                let center =
                    rect.min + egui::vec2(5.0 + col as f32 * 6.0, 4.0 + row as f32 * 6.0);
                ui.painter().circle_filled(center, 1.2, visuals.fg_stroke.color);
            }
        }
        resp.clone().on_hover_text("Drag out as its own window");
        Self::arm_tab_drag(&resp, window_name, tab_state);
    }

    /// Make a tab a drag source: egui sets the payload only once the press
    /// has travelled its click/drag threshold, so a plain click never
    /// tears out. The drop is consumed in `poll_tab_drag`.
    fn arm_tab_drag(
        resp: &egui::Response,
        window_name: &str,
        tab_state: &crate::data::TabState,
    ) {
        resp.dnd_set_drag_payload(super::super::tab_drag::TabDragPayload {
            window: window_name.to_string(),
            tab_id: tab_state.definition.id,
            label: tab_state.definition.name.clone(),
        });
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

        // A game link (Ctrl+drag) held over the bar can be dropped to make
        // a button for it: the band shows while the drag is over the bar,
        // and the release hands the shell a request to add the button.
        if let Some(link) = egui::DragAndDrop::payload::<LinkData>(ui.ctx()) {
            let bar_rect = ui.min_rect().expand(4.0);
            if ui.rect_contains_pointer(bar_rect) && Self::link_is_draggable(&link) {
                ui.painter().rect(
                    bar_rect,
                    4.0,
                    Color32::from_rgba_unmultiplied(70, 180, 90, 50),
                    egui::Stroke::new(1.5, Color32::from_rgb(90, 200, 110)),
                    egui::StrokeKind::Inside,
                );
                if let Some(pos) = ui.input(|input| input.pointer.latest_pos()) {
                    egui::Area::new(egui::Id::new(("hotbar_drop_hint", bar_name)))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(pos + egui::vec2(16.0, 16.0))
                        .interactable(false)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.label(format!(
                                    "Release to add a '{}' button",
                                    link.text.trim()
                                ));
                            });
                        });
                }
                if ui.input(|input| input.pointer.any_released()) && clicked.is_none() {
                    let label = if link.text.trim().is_empty() {
                        link.noun.clone()
                    } else {
                        link.text.trim().to_string()
                    };
                    clicked = Some(GuiLinkClick {
                        link_data: LinkData {
                            exist_id: Self::HOTBAR_DROP_SENTINEL.to_string(),
                            noun: format!(
                                "{}|{}|{}|{}",
                                bar_name,
                                link.exist_id.trim_start_matches('#'),
                                link.noun,
                                label
                            ),
                            text: String::new(),
                            coord: None,
                        },
                        click_pos: (0, 0),
                    });
                }
            }
        }
        clicked
    }

    /// Add a button for a dropped game link to `bar_name` and persist it in
    /// whichever hotbars file (character or global) defines the bar.
    pub(in crate::frontend::gui::app) fn add_hotbar_button_for_link(
        app_core: &mut AppCore,
        bar_name: &str,
        exist_id: &str,
        noun: &str,
        label: &str,
    ) {
        use crate::config::{Config, HotbarButton, HotbarsConfig};
        let Some(mut bar) = app_core.config.hotbars.find_bar(bar_name).cloned() else {
            app_core.add_system_message(&format!("Hotbar '{bar_name}' is not defined."));
            return;
        };
        let character = app_core.config.character.clone();
        // The bar lives in the character file if that file defines it,
        // otherwise in the global one (the same merge order the loader uses).
        let is_global = !Config::hotbars_path(character.as_deref())
            .ok()
            .filter(|path| path.exists())
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| toml::from_str::<HotbarsConfig>(&text).ok())
            .is_some_and(|cfg| cfg.find_bar(bar_name).is_some());
        let mut id = format!("link_{exist_id}");
        while bar.buttons.iter().any(|b| b.id == id) {
            id.push('x');
        }
        let command = if noun.trim().is_empty() {
            format!(".menuof {exist_id}")
        } else {
            format!(".menuof {exist_id} {noun}")
        };
        bar.buttons.push(HotbarButton {
            id,
            label: label.to_string(),
            command,
            tooltip: Some(format!("Menu for {label} (as clicking its link)")),
            ..Default::default()
        });
        match Config::save_hotbar(&bar, is_global, character.as_deref()) {
            Ok(()) => {
                app_core.reload_hotbars();
                app_core.add_system_message(&format!(
                    "Added '{label}' to hotbar '{bar_name}'. Edit or remove it in .hotbars."
                ));
            }
            Err(err) => {
                app_core.add_system_message(&format!("Failed to save hotbar '{bar_name}': {err}"));
            }
        }
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
