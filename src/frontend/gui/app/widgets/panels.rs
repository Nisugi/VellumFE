//! Assorted panel widgets: hands, experience (GS4/DR), encumbrance,
//! betrayer, perception, items, dialog panels with their skinned control
//! painters, and container windows.

use super::*;

impl VellumGuiApp {
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
    pub(in crate::frontend::gui::app) fn gs4_experience_flags(
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
    pub(in crate::frontend::gui::app) fn encumbrance_flags(app_core: &AppCore, window_name: &str) -> (bool, bool) {
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
    pub(super) fn format_thousands(value: u64) -> String {
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
        // Cross-id content pairs (ESP: window espMasterDialog, controls
        // espMasterData): fall through to the alias slot when the bound
        // slot has nothing to draw.
        let store = &app_core.ui_state.dialog_store;
        let primary = store.get(dialog_id);
        let alias = crate::core::local_catalog::dialog_content_alias(dialog_id)
            .and_then(|alias_id| store.get(alias_id));
        let dialog = match (primary, alias) {
            (Some(d), _) if d.positioned_controls().is_some() || !d.buttons.is_empty() => d,
            (_, Some(d)) => d,
            (Some(d), None) => d,
            (None, None) => {
                ui.weak("Waiting for the game to send this panel…");
                return;
            }
        };
        let queue = |cmd: String| {
            if !cmd.trim().is_empty() {
                app_core.ui_state.pending_panel_commands.borrow_mut().push(cmd);
            }
        };

        // Spinbox edits live in egui temp memory (this path is immutable);
        // every command resolves against a probe carrying those edits, so
        // 'withdraw %withdrawSB%' sends what the user dialed in.
        let spin_mem =
            |id: &str| egui::Id::new(("panel_spin", dialog_id.to_string(), id.to_string()));
        let patched_command = |ui: &egui::Ui, cmd: &str| -> String {
            let mut probe = dialog.clone();
            for spin in probe.spinboxes.iter_mut() {
                if let Some(v) = ui.ctx().data(|d| d.get_temp::<i32>(spin_mem(&spin.id))) {
                    spin.value = v;
                }
            }
            probe.command_with_placeholders(cmd)
        };

        let positioned = dialog.positioned_controls();
        let (content_w, content_h) = positioned
            .as_ref()
            .map(|(_, size)| *size)
            .unwrap_or((190.0, 24.0));
        let (canvas_rect, _) =
            ui.allocate_exact_size(egui::vec2(content_w, content_h), egui::Sense::hover());
        let origin = canvas_rect.min;

        // Positioned commanded images the canvas loop actually painted (skin
        // sprite found). Any commanded image NOT in here must still surface in
        // the footer fallback below, or the command is unreachable — the
        // default skin-less install has no sprites at all.
        let mut drawn_images: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        if let Some((controls, _)) = &positioned {
            use crate::data::ui_state::PositionedControlKind;
            for control in controls {
                let (x, y, w, h) = control.rect;
                let rect =
                    egui::Rect::from_min_size(origin + egui::vec2(x, y), egui::vec2(w, h));
                match control.kind {
                    PositionedControlKind::Button(i) => {
                        if let Some(b) = dialog.buttons.get(i) {
                            let resp = Self::skinned_panel_button(
                                ui,
                                rect,
                                &b.label,
                                ("panel_btn", dialog_id, i),
                                skin_art,
                            );
                            if resp.clicked() {
                                if b.is_close {
                                    // Wrayth's closeButton dismisses the
                                    // hosting window; routed through the
                                    // panel-command drain as a client verb.
                                    queue(format!("__VELLUM_CLOSE_PANEL__{dialog_id}"));
                                } else {
                                    queue(patched_command(ui, &b.command));
                                }
                            }
                        }
                    }
                    PositionedControlKind::DropDown(i) => {
                        if let Some(d) = dialog.dropdowns.get(i) {
                            // Skinned dropdown frame behind the combo box (falls
                            // back to the dropdown's own theme frame when absent).
                            if let Some(border) =
                                skin_art.and_then(|art| art.control_border("dropdown", "normal"))
                            {
                                crate::frontend::gui::skin::paint_nine_slice_filled(
                                    ui.painter(),
                                    rect,
                                    border,
                                );
                            }
                            if let Some(value) =
                                Self::dialog_panel_combo(ui, rect, dialog_id, d, skin_art)
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
                            Self::paint_panel_progress_bar(ui, rect, bar, skin_art);
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
                            // The InjuriesPanel doll is the character's own,
                            // so variants and hidden parts apply here too.
                            let (doll_variant, doll_hidden) =
                                Self::resolve_doll_render(app_core, skin_art);
                            Self::paint_dialog_skin(
                                ui,
                                rect,
                                skin,
                                dialog,
                                skin_art,
                                doll_variant,
                                &doll_hidden,
                            );
                        }
                    }
                    PositionedControlKind::Link(i) => {
                        // Bottom-floor links (combat's `skin` at top=260) render
                        // in the footer row below, not here — skip them so they
                        // don't draw twice / crowd the footer.
                        let bottom_floor = dialog
                            .links
                            .get(i)
                            .and_then(|l| l.layout.as_ref())
                            .and_then(|layout| layout.top)
                            .is_some_and(|top| top as f32 >= content_h - PANEL_FOOTER_BAND);
                        if bottom_floor {
                            continue;
                        }
                        if let Some(link) = dialog.links.get(i) {
                            // A skin may give link "buttons" their own art; when
                            // it does, paint the state-keyed nine-slice behind the
                            // label. Otherwise it stays plain hyperlink text.
                            let link_art =
                                skin_art.and_then(|art| art.control_border("link", "normal"));
                            if link_art.is_some() {
                                let sense = ui.interact(
                                    rect,
                                    ui.id().with(("panel_link", dialog_id, i)),
                                    egui::Sense::click(),
                                );
                                let state = if sense.is_pointer_button_down_on() {
                                    "pressed"
                                } else if sense.hovered() {
                                    "hover"
                                } else {
                                    "normal"
                                };
                                if let Some(border) =
                                    skin_art.and_then(|art| art.control_border("link", state))
                                {
                                    crate::frontend::gui::skin::paint_nine_slice_filled(
                                        ui.painter(),
                                        rect,
                                        border,
                                    );
                                }
                            }
                            let text = egui::RichText::new(&link.label)
                                .color(ui.visuals().hyperlink_color);
                            if ui
                                .put(rect, egui::Button::new(text).small().frame(false))
                                .clicked()
                            {
                                queue(patched_command(ui, &link.command));
                            }
                        }
                    }
                    PositionedControlKind::SpinBox(i) => {
                        if let Some(spin) = dialog.spinboxes.get(i) {
                            let mem = spin_mem(&spin.id);
                            let mut value = ui
                                .ctx()
                                .data_mut(|d| *d.get_temp_mut_or(mem, spin.value));
                            let range = spin.min..=spin.max.max(spin.min);
                            let resp = ui.put(
                                rect,
                                egui::DragValue::new(&mut value).range(range).speed(25),
                            );
                            if resp.changed() {
                                ui.ctx().data_mut(|d| d.insert_temp(mem, value));
                            }
                        }
                    }
                    PositionedControlKind::Image(i) => {
                        // Icon buttons (Wrayth SwordBtn/ShieldBtn/...) are the
                        // ONLY images drawn as controls: they carry a command AND
                        // a real on-screen size. Everything else in the image list
                        // is an ANCHOR POINT — zero-size wound points and the
                        // invisible PanelBackground the vitals bars hang from —
                        // and must never draw (else it bleeds behind neighbors).
                        let drawable = rect.width() >= 1.0 && rect.height() >= 1.0;
                        if let (true, Some(img)) = (drawable, dialog.images.get(i)) {
                            let has_cmd = !img.command.trim().is_empty();
                            let sprite =
                                skin_art.and_then(|art| art.icon(&img.name.to_ascii_lowercase()));
                            if let Some(icon) = sprite {
                                drawn_images.insert(i);
                                let resp = ui.interact(
                                    rect,
                                    ui.id().with(("panel_img", dialog_id, i)),
                                    if has_cmd {
                                        egui::Sense::click()
                                    } else {
                                        egui::Sense::hover()
                                    },
                                );
                                let dest =
                                    crate::frontend::gui::skin::icon_dest(&icon, rect);
                                crate::frontend::gui::skin::paint_icon(
                                    ui.painter(),
                                    dest,
                                    &icon,
                                    egui::Color32::WHITE,
                                );
                                // A multiply tint can't brighten past the
                                // sprite's own colors, so hover feedback is a
                                // translucent wash over the icon instead.
                                if has_cmd && resp.hovered() {
                                    ui.painter().rect_filled(
                                        dest,
                                        3.0,
                                        egui::Color32::from_white_alpha(24),
                                    );
                                }
                                if has_cmd {
                                    let resp = img
                                        .tooltip
                                        .as_deref()
                                        .map(|t| resp.clone().on_hover_text(t))
                                        .unwrap_or(resp);
                                    if resp.clicked() {
                                        queue(dialog.command_with_placeholders(&img.command));
                                    }
                                }
                            }
                            // No sprite for a positioned image: draw nothing
                            // here (a stray button behind neighboring controls
                            // is worse) — it's not in `drawn_images`, so the
                            // footer's labeled-button fallback picks it up and
                            // the command stays reachable even skin-less.
                        }
                    }
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
        // Images the canvas loop drew as positioned icon buttons are excluded
        // (they'd render twice); everything else commanded lands here. Keying
        // on `drawn_images` — NOT `layout.is_some()` — matters: a positioned
        // image with no matching skin sprite (skin-less runs, or a skin
        // missing that icon) draws nothing above, and this footer is the only
        // way its command stays clickable.
        let footer_images: Vec<_> = dialog
            .images
            .iter()
            .enumerate()
            .filter(|(_, image)| !image.command.trim().is_empty())
            .filter(|(_, image)| !doll_owned.contains(image.id.as_str()))
            .filter(|(idx, _)| !drawn_images.contains(idx))
            .map(|(_, image)| image)
            .collect();
        if !footer_images.is_empty() {
            let has_btn_art = skin_art
                .and_then(|art| art.control_border("button", "normal"))
                .is_some();
            ui.horizontal_wrapped(|ui| {
                for (i, image) in footer_images.iter().enumerate() {
                    let label = image.tooltip.as_deref().unwrap_or(&image.name);
                    let clicked = if has_btn_art {
                        // Allocate a content-sized rect so the skinned button
                        // background stretches to the label, then paint it.
                        let galley = ui.painter().layout_no_wrap(
                            label.to_string(),
                            egui::FontId::proportional(13.0),
                            ui.visuals().text_color(),
                        );
                        let size = galley.size() + egui::vec2(14.0, 6.0);
                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        Self::skinned_panel_button(
                            ui,
                            rect,
                            label,
                            ("panel_footer_btn", dialog_id, i),
                            skin_art,
                        )
                        .clicked()
                    } else {
                        ui.small_button(label).clicked()
                    };
                    if clicked {
                        queue(dialog.command_with_placeholders(&image.command));
                    }
                }
            });
        }
        // Footer row: links WITHOUT layout data (combat's search/grip/multistrike
        // line) plus bottom-anchored positioned links. Wrayth positions a couple
        // of links near the panel floor (combat's `skin` at top=260); at
        // VellumFE's content height they land right on top of this row, so links
        // positioned in the bottom ~40px of the canvas are treated as footer
        // links here instead of drawn in the canvas — keeping the footer a single
        // clean row. The `has_positions` canvas still owns everything above.
        let floor = content_h - PANEL_FOOTER_BAND;
        let footer_links: Vec<_> = dialog
            .links
            .iter()
            .filter(|l| {
                l.layout.is_none()
                    || l.layout
                        .as_ref()
                        .and_then(|layout| layout.top)
                        .is_some_and(|top| top as f32 >= floor)
            })
            .collect();
        if !footer_links.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for link in footer_links {
                    if ui.link(&link.label).clicked() {
                        queue(patched_command(ui, &link.command));
                    }
                }
            });
        }
    }

    /// Paint a dialog progress bar to its EXACT resolved rect (Wrayth pixel
    /// layout), instead of ui.put'ing an egui ProgressBar that centers itself
    /// at its own min-size and overflows the 15px rows UberBar uses. Trough +
    /// fill (fraction of width) + centered customText.
    /// A dialog-panel button that honors the skin's `[controls.button]`
    /// nine-slice (state-keyed) when present, falling back to a plain egui
    /// button. Returns the response so callers handle clicks uniformly.
    pub(super) fn skinned_panel_button(
        ui: &mut egui::Ui,
        rect: egui::Rect,
        label: &str,
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> egui::Response {
        if skin_art
            .and_then(|art| art.control_border("button", "normal"))
            .is_some()
        {
            let resp = ui.interact(rect, ui.id().with(id_salt), egui::Sense::click());
            let state = if resp.is_pointer_button_down_on() {
                "pressed"
            } else if resp.hovered() {
                "hover"
            } else {
                "normal"
            };
            if let Some(border) = skin_art.and_then(|art| art.control_border("button", state)) {
                // Filled: a button FACE paints its center from the sprite —
                // the hollow window-frame variant let the dark window mesh
                // show through the middle of every button.
                crate::frontend::gui::skin::paint_nine_slice_filled(ui.painter(), rect, border);
            }
            // PAINT the label directly over the nine-slice art — do NOT place an
            // egui Button, which draws its own dark widget background (button_bg
            // / hover fill) even frameless+transparent, leaving a black box on
            // top of the silver button sprite. Color follows button_text
            // (defaults to body text); the skin pins it dark so it reads on a
            // light button.
            let color = skin_art
                .and_then(|art| art.ui_palette.as_ref())
                .map(|pal| pal.button_text)
                .unwrap_or_else(|| ui.visuals().text_color());
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional((rect.height() * 0.6).clamp(9.0, 14.0)),
                color,
            );
            resp
        } else {
            ui.put(rect, egui::Button::new(label).small())
        }
    }

    pub(super) fn paint_panel_progress_bar(
        ui: &egui::Ui,
        rect: egui::Rect,
        bar: &crate::data::DialogProgressBar,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) {
        let painter = ui.painter();
        let visuals = ui.visuals();
        let radius = 2.0;
        let frame = skin_art.and_then(|art| art.control_border("progressbar", "normal"));
        // Trough: the skin's nine-slice frame if present, else a filled rect.
        if frame.is_none() {
            painter.rect_filled(rect, radius, visuals.extreme_bg_color);
        }
        // Fill (color from the game feed; the skin frames it, doesn't recolor).
        let frac = (bar.value.min(100) as f32 / 100.0).clamp(0.0, 1.0);
        if frac > f32::EPSILON {
            let mut fill_rect = rect;
            fill_rect.set_width(rect.width() * frac);
            painter.rect_filled(fill_rect, radius, widget_accent(ui.ctx(), visuals));
        }
        // Skin frame paints on top so its border edges the fill.
        if let Some(border) = frame {
            crate::frontend::gui::skin::paint_nine_slice(painter, rect, border, [true; 4]);
        }
        // Centered text (auto-contrast against the ground it sits on).
        if !bar.text.is_empty() {
            let behind = if frac >= 0.5 {
                widget_accent(ui.ctx(), visuals)
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
    /// (bitfield; see [`crate::data::DialogLabel::align`]). UberBar
    /// right-justifies its value columns; ui.put centered them mid-slot,
    /// which read as floating gaps.
    pub(super) fn paint_panel_label(
        ui: &egui::Ui,
        rect: egui::Rect,
        label: &crate::data::DialogLabel,
    ) {
        if label.value.is_empty() {
            return;
        }
        let (anchor, pos) = match label.align() {
            crate::data::LabelAlign::Right => (egui::Align2::RIGHT_CENTER, rect.right_center()),
            crate::data::LabelAlign::Center => (egui::Align2::CENTER_CENTER, rect.center()),
            crate::data::LabelAlign::Left => (egui::Align2::LEFT_CENTER, rect.left_center()),
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
    pub(super) fn paint_dialog_skin(
        ui: &mut egui::Ui,
        rect: egui::Rect,
        skin: &crate::data::DialogSkin,
        dialog: &crate::data::ui_state::DialogState,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
        doll_variant: Option<usize>,
        doll_hidden: &std::collections::HashSet<String>,
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
                doll_variant,
                doll_hidden,
                false,
                &Self::default_injury_palette(),
            );
        });
    }

    /// A ComboBox for a dialog-panel dropdown; returns the newly picked
    /// value. Mirrors the popup dialog's dropdown_combo.
    pub(super) fn dialog_panel_combo(
        ui: &mut egui::Ui,
        rect: egui::Rect,
        dialog_id: &str,
        dropdown: &crate::data::DialogDropDown,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> Option<String> {
        let selected_text = dropdown
            .options
            .iter()
            .find(|(_, value)| *value == dropdown.value)
            .map(|(text, _)| text.clone())
            .unwrap_or_else(|| dropdown.value.clone());
        let mut picked = None;
        // A skinned dropdown themes its OPEN popup too: egui paints the popup
        // frame from window_fill/stroke, so match them to the skin's dropdown
        // border (its tex_size carries no color, so we key on presence and use
        // a dark fill + accent-tinted stroke that reads with the mesh grounds).
        let skin_popup = skin_art
            .and_then(|art| art.control_border("dropdown", "normal"))
            .is_some();
        // The skinned dropdown popup's fill/stroke follow the active skin's
        // [ui] palette (menu_bg, border, button_hover, accent) — not a hardcoded
        // orange, which looked wrong on any non-orange skin (StormFront's
        // steel-blue dropdown came out orange). Falls back to the previous
        // fixed colors only when a dropdown is skinned but the palette is
        // somehow absent.
        let popup_palette = skin_art.and_then(|art| art.ui_palette.as_ref());
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            if skin_popup {
                let v = ui.visuals_mut();
                let (fill, stroke, hover, sel) = match popup_palette {
                    Some(p) => (p.menu_bg, p.border, p.button_hover, p.accent),
                    None => (
                        egui::Color32::from_rgb(0x12, 0x14, 0x18),
                        egui::Color32::from_rgb(0x8a, 0x5a, 0x30),
                        egui::Color32::from_rgb(0x24, 0x1c, 0x12),
                        egui::Color32::from_rgb(0x3a, 0x28, 0x14),
                    ),
                };
                v.window_fill = fill;
                v.panel_fill = fill;
                v.window_stroke = egui::Stroke::new(1.0, stroke);
                v.widgets.inactive.weak_bg_fill = fill;
                v.widgets.hovered.weak_bg_fill = hover;
                v.selection.bg_fill = sel;
            }
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
    pub(in crate::frontend::gui::app) const QUICKBAR_SWITCH_SENTINEL: &'static str = "_quickbar_switch_";

}
