//! Containers window: the managed-inventory tree (extended feed snapshot)
//! rendered as collapsible containers with live capacity and open/closed
//! state. Read-only over `GameState.managed_inventory`; refresh is the
//! manual `.invsync` (owner decision - no auto-refresh), and item actions
//! ride the verified `.drag` command.

use super::*;

impl VellumGuiApp {
    pub(super) fn render_containers_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
    ) -> Option<GuiLinkClick> {
        let mut clicked: Option<GuiLinkClick> = None;
        let mut click = |ui: &egui::Ui, response: &egui::Response, cmd: String| {
            Some(Self::gui_link_click_from_response(
                response,
                ui,
                Self::direct_command_link(cmd),
            ))
        };

        // Header: refresh + snapshot vintage.
        let snapshot = app_core.game_state.managed_inventory.as_ref();
        ui.horizontal(|ui| {
            let refresh = ui
                .button("⟳ Refresh")
                .on_hover_text("Request a fresh snapshot (.invsync). Manual by design.");
            if refresh.clicked() {
                clicked = click(ui, &refresh, ".invsync".to_string());
            }
            match snapshot {
                Some(snap) => {
                    ui.weak(format!(
                        "{} items{}",
                        snap.items.len(),
                        if snap.complete { "" } else { " (incomplete)" }
                    ));
                }
                None => {
                    ui.weak("no snapshot yet");
                }
            }
        });
        let Some(snap) = snapshot else {
            ui.separator();
            ui.label("Refresh to load the structured inventory.");
            ui.weak("Needs the extended feed: the game must see the WRAYTH banner (direct mode sends it; Lich can too).");
            return clicked;
        };
        ui.separator();

        // Index children by parent id, preserving feed order.
        let mut children: std::collections::HashMap<&str, Vec<&crate::core::state::ManagedInventoryItem>> =
            std::collections::HashMap::new();
        for item in &snap.items {
            children.entry(item.parent.as_str()).or_default().push(item);
        }
        // Saga-accounting weights (own + contents, 0.1lb floor, deep-container
        // rule) and nested descendant counts, computed once per frame.
        let weights = snap.weight_breakdowns();
        let counts = snap.descendant_counts();

        // Roots split by the wire's relation vocabulary, with containers
        // separated from plain items inside worn and room - the sections
        // you actually think in.
        let group_of = |item: &crate::core::state::ManagedInventoryItem| -> usize {
            let has_kids = children
                .get(item.id.as_str())
                .is_some_and(|k| !k.is_empty());
            let container = item.is_container() || has_kids;
            match item.relation.as_str() {
                "righthand" | "lefthand" => 0,
                "worn" if container => 1,
                "worn" => 2,
                "atfeet" => 3,
                "reserved" => 4,
                _ if container => 5, // room containers
                _ => 6,              // loose ground items
            }
        };
        const GROUPS: [&str; 7] = [
            "Hands",
            "Worn containers",
            "Worn",
            "At your feet",
            "Reserved",
            "Room containers",
            "On the ground",
        ];
        let mut roots: [Vec<&crate::core::state::ManagedInventoryItem>; 7] = Default::default();
        for item in &snap.items {
            if item.parent == "player" || item.parent == "room" {
                roots[group_of(item)].push(item);
            }
        }

        // Room-relation items belong to the room the snapshot was taken in;
        // after walking away they're stale scenery, not your surroundings.
        let room_stale = app_core
            .message_processor
            .current_room_uid()
            .is_some_and(|uid| uid.to_string() != snap.room);
        let heading_band = widget_accent(ui.ctx(), ui.visuals());
        let heading_color = if super::super::theme::relative_luminance(heading_band) > 0.5 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };

        // Tab bar: Containers (the trees you dig through), Worn (flat
        // trinket list), Room, Item (full-height inspector). Clicking an
        // item anywhere jumps to the Item tab.
        let tab_key = Self::containers_tab_key();
        let mut tab: u8 = ui.ctx().data(|d| d.get_temp(tab_key)).unwrap_or(0);
        let inspector = app_core.game_state.viewed_item.as_ref();
        ui.horizontal(|ui| {
            for (i, label) in ["Containers", "Worn", "Room", "Item"].iter().enumerate() {
                if ui.selectable_label(tab == i as u8, *label).clicked() {
                    tab = i as u8;
                    ui.ctx().data_mut(|d| d.insert_temp(tab_key, tab));
                }
            }
        });
        ui.separator();

        // Group indices per tab (from the GROUPS split above).
        let tab_groups: &[usize] = match tab {
            0 => &[0, 1, 3, 4], // hands, worn containers, at feet, reserved
            1 => &[2],          // worn non-containers
            _ => &[5, 6],       // room containers, on the ground
        };

        if tab == 3 {
            // Full-height inspector.
            match inspector {
                Some(view) => {
                    ui.label(
                        egui::RichText::new(format!(" {} ", view.name))
                            .strong()
                            .color(heading_color)
                            .background_color(heading_band),
                    );
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical()
                        .id_salt("containers_inspector")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (command, text) in &view.results {
                                if text.trim().is_empty() {
                                    continue;
                                }
                                ui.label(
                                    egui::RichText::new(format!(" {} ", command.to_uppercase()))
                                        .small()
                                        .color(heading_color)
                                        .background_color(heading_band),
                                );
                                ui.label(text);
                                ui.add_space(6.0);
                            }
                            if view.results.iter().all(|(_, t)| t.trim().is_empty()) {
                                ui.weak("nothing further to see");
                            }
                        });
                }
                None => {
                    ui.weak("Click an item (or right-click > Inspect) to view it here.");
                }
            }
            return clicked;
        }

        // Focus mode (Containers tab only): one container as a flat list.
        // Focus id lives in egui temp data (cleared by the back button or a
        // vanished id).
        if tab == 0 {
            let focus_key = egui::Id::new("containers_focus");
            let focus_id: Option<String> = ui.ctx().data(|d| d.get_temp(focus_key));
            if let Some(fid) = focus_id.as_deref() {
                if let Some(focused) = snap.items.iter().find(|i| i.id == fid) {
                    return Self::render_focused_container(
                        app_core, ui, snap, focused, &children, &weights, &counts, focus_key,
                        heading_band, heading_color, clicked, &mut click,
                    );
                }
                // Focused container no longer in the snapshot: drop focus.
                ui.ctx().data_mut(|d| d.remove_temp::<String>(focus_key));
            }
        }

        egui::ScrollArea::vertical()
            .id_salt("containers_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut room_items_hidden = false;
                let mut anything = false;
                for &gi in tab_groups {
                    let group = &roots[gi];
                    if group.is_empty() {
                        continue;
                    }
                    // Room-relation groups: suppress when stale.
                    if gi >= 5 && room_stale {
                        room_items_hidden = true;
                        continue;
                    }
                    anything = true;
                    // Single-section tabs skip the redundant heading.
                    if tab_groups.len() > 1 {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!(" {} ", GROUPS[gi]))
                                .strong()
                                .color(heading_color)
                                .background_color(heading_band),
                        );
                        ui.separator();
                    }
                    for item in group {
                        Self::containers_node(
                            ui, item, &children, &weights, &counts, 0, &mut clicked, &mut click,
                        );
                    }
                }
                if room_items_hidden {
                    ui.add_space(6.0);
                    ui.weak("Room items are from another room - Refresh here to load them.");
                } else if !anything {
                    ui.weak("nothing here");
                }
            });
        clicked
    }

    /// egui temp-data key for the active Containers-window tab.
    fn containers_tab_key() -> egui::Id {
        egui::Id::new("containers_tab")
    }

    /// Jump the Containers window to the Item (inspector) tab.
    fn containers_show_item_tab(ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(Self::containers_tab_key(), 3u8));
    }

    /// Focus mode: one container's entire subtree as a flat, name-sorted
    /// list with a back button. The digging-through-one-bag view.
    #[allow(clippy::too_many_arguments)]
    fn render_focused_container(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        snap: &crate::core::state::ManagedInventoryState,
        focused: &crate::core::state::ManagedInventoryItem,
        children: &std::collections::HashMap<&str, Vec<&crate::core::state::ManagedInventoryItem>>,
        weights: &std::collections::HashMap<String, crate::core::state::WeightBreakdown>,
        counts: &std::collections::HashMap<String, usize>,
        focus_key: egui::Id,
        heading_band: Color32,
        heading_color: Color32,
        mut clicked: Option<GuiLinkClick>,
        click: &mut impl FnMut(&egui::Ui, &egui::Response, String) -> Option<GuiLinkClick>,
    ) -> Option<GuiLinkClick> {
        let _ = app_core;
        if ui.button("◀ All containers").clicked() {
            ui.ctx().data_mut(|d| d.remove_temp::<String>(focus_key));
        }
        let bd = weights.get(focused.id.as_str()).copied().unwrap_or_default();
        let count = counts.get(focused.id.as_str()).copied().unwrap_or(0);
        ui.label(
            egui::RichText::new(format!(
                "{} — {count} item{} · {} lbs",
                focused.name,
                if count == 1 { "" } else { "s" },
                Self::fmt_lbs(bd.total)
            ))
            .strong()
            .color(heading_color)
            .background_color(heading_band),
        );
        ui.separator();

        // Flatten the subtree (cycle-bounded), then sort by name.
        let mut flat: Vec<&crate::core::state::ManagedInventoryItem> = Vec::new();
        let mut stack: Vec<(&crate::core::state::ManagedInventoryItem, usize)> =
            children
                .get(focused.id.as_str())
                .into_iter()
                .flatten()
                .map(|i| (*i, 0))
                .collect();
        while let Some((item, depth)) = stack.pop() {
            if depth > 16 {
                continue;
            }
            flat.push(item);
            for kid in children.get(item.id.as_str()).into_iter().flatten() {
                stack.push((kid, depth + 1));
            }
        }
        flat.sort_by(|a, b| a.noun.cmp(&b.noun).then(a.name.cmp(&b.name)));

        egui::ScrollArea::vertical()
            .id_salt("containers_focus_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if flat.is_empty() {
                    ui.weak(if focused.is_closed() { "closed" } else { "empty" });
                }
                for item in &flat {
                    let weight = if item.weight > 0 {
                        format!("  ({} lb{})", item.weight, if item.weight == 1 { "" } else { "s" })
                    } else {
                        String::new()
                    };
                    // Items nested deeper than the focused bag get their
                    // holder appended so the flat list stays unambiguous.
                    let holder = if item.parent != focused.id {
                        snap.items
                            .iter()
                            .find(|p| p.id == item.parent)
                            .map(|p| format!("  · in {}", p.noun))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let response = ui
                        .add(
                            egui::Label::new(format!("{}{weight}{holder}", item.name))
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text(item.long.as_deref().unwrap_or(&item.name));
                    if response.clicked() && clicked.is_none() {
                        Self::containers_show_item_tab(ui.ctx());
                        clicked = click(ui, &response, format!(".viewitem {}", item.id));
                    }
                    Self::containers_context_menu(ui, &response, item, &mut clicked, click);
                }
            });
        clicked
    }

    /// Format an optional pounds value ("unknown" for None, trim .0).
    fn fmt_lbs(v: Option<f32>) -> String {
        match v {
            None => "unknown".to_string(),
            Some(v) if (v - v.round()).abs() < f32::EPSILON => format!("{}", v.round() as i64),
            Some(v) => format!("{v:.1}"),
        }
    }

    /// One item row; containers recurse as collapsible headers.
    #[allow(clippy::too_many_arguments)]
    fn containers_node(
        ui: &mut egui::Ui,
        item: &crate::core::state::ManagedInventoryItem,
        children: &std::collections::HashMap<&str, Vec<&crate::core::state::ManagedInventoryItem>>,
        weights: &std::collections::HashMap<String, crate::core::state::WeightBreakdown>,
        counts: &std::collections::HashMap<String, usize>,
        depth: usize,
        clicked: &mut Option<GuiLinkClick>,
        click: &mut impl FnMut(&egui::Ui, &egui::Response, String) -> Option<GuiLinkClick>,
    ) {
        // Runaway guard for malformed parent cycles.
        if depth > 16 {
            return;
        }
        let kids = children.get(item.id.as_str());
        let is_container = item.is_container() || kids.is_some_and(|k| !k.is_empty());

        if is_container {
            // Header label: state glyph + name + nested count, carried
            // weight (own + contents, Saga accounting), capacity.
            let glyph = if item.is_locked() {
                "🔒 "
            } else if item.is_closed() {
                "▣ "
            } else {
                ""
            };
            let count = if item.is_locked() {
                "locked".to_string()
            } else {
                let n = counts.get(item.id.as_str()).copied().unwrap_or(0);
                format!("{n} item{}", if n == 1 { "" } else { "s" })
            };
            let bd = weights.get(item.id.as_str()).copied().unwrap_or_default();
            let weight = format!("{} lbs", Self::fmt_lbs(bd.total));
            let capacity = match item.in_capacity() {
                Some(cap) => format!("{} cap", cap.pounds),
                None => String::new(),
            };
            // Custom header row: name on the left, stats in fixed-width
            // right-aligned columns (items | lbs | cap) so every row's
            // numbers line up — a plain CollapsingHeader title can't
            // column-align in a proportional font.
            let state_id = ui.make_persistent_id(("containers_node", item.id.as_str()));
            let cstate = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                state_id,
                false,
            );
            let (response, _, _) = cstate
                .show_header(ui, |ui| {
                    ui.label(format!("{glyph}{}", item.name));
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let col = |ui: &mut egui::Ui, text: String, width: f32| {
                                ui.allocate_ui_with_layout(
                                    egui::Vec2::new(width, 16.0),
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(egui::RichText::new(text).weak());
                                    },
                                );
                            };
                            // Right-to-left: rightmost column first.
                            col(ui, capacity.clone(), 78.0);
                            col(ui, weight.clone(), 64.0);
                            col(ui, count.clone(), 64.0);
                        },
                    );
                })
                .body(|ui| {
                    if let Some(kids) = kids {
                        for kid in kids {
                            Self::containers_node(
                                ui, kid, children, weights, counts, depth + 1, clicked, click,
                            );
                        }
                    } else if item.is_closed() {
                        ui.weak("closed");
                    } else {
                        ui.weak("empty");
                    }
                });
            let response = response.on_hover_text(format!(
                "Container: {} lbs, Contents: {} lbs",
                Self::fmt_lbs(bd.own),
                Self::fmt_lbs(bd.contents)
            ));
            Self::containers_context_menu(ui, &response, item, clicked, click);
        } else {
            let weight = if item.weight > 0 {
                format!("  ({} lb{})", item.weight, if item.weight == 1 { "" } else { "s" })
            } else {
                String::new()
            };
            let response = ui
                .add(egui::Label::new(format!("{}{weight}", item.name)).sense(egui::Sense::click()))
                .on_hover_text(item.long.as_deref().unwrap_or(&item.name));
            // Left-click loads the item into the Item tab.
            if response.clicked() && clicked.is_none() {
                Self::containers_show_item_tab(ui.ctx());
                *clicked = click(ui, &response, format!(".viewitem {}", item.id));
            }
            Self::containers_context_menu(ui, &response, item, clicked, click);
        }
    }

    /// Right-click actions: look, verified .drag moves.
    fn containers_context_menu(
        ui: &egui::Ui,
        response: &egui::Response,
        item: &crate::core::state::ManagedInventoryItem,
        clicked: &mut Option<GuiLinkClick>,
        click: &mut impl FnMut(&egui::Ui, &egui::Response, String) -> Option<GuiLinkClick>,
    ) {
        let mut chosen: Option<String> = None;
        response.context_menu(|menu_ui| {
            menu_ui.label(&item.name);
            menu_ui.separator();
            if menu_ui.button("Inspect").clicked() {
                Self::containers_show_item_tab(menu_ui.ctx());
                chosen = Some(format!(".viewitem {}", item.id));
                menu_ui.close();
            }
            if menu_ui.button("Look").clicked() {
                chosen = Some(format!("look #{}", item.id));
                menu_ui.close();
            }
            if item.can_pick_up() {
                if menu_ui.button("To right hand").clicked() {
                    chosen = Some(format!(".drag {} right", item.id));
                    menu_ui.close();
                }
                if menu_ui.button("To left hand").clicked() {
                    chosen = Some(format!(".drag {} left", item.id));
                    menu_ui.close();
                }
                menu_ui.separator();
                if menu_ui.button("Drop").clicked() {
                    chosen = Some(format!(".drag {} drop", item.id));
                    menu_ui.close();
                }
                if menu_ui.button("Wear").clicked() {
                    chosen = Some(format!(".drag {} wear", item.id));
                    menu_ui.close();
                }
            }
            if item.is_container() {
                menu_ui.separator();
                if menu_ui.button("Focus").clicked() {
                    menu_ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("containers_focus"), item.id.clone())
                    });
                    menu_ui.close();
                }
                if item.is_closed() {
                    if menu_ui.button("Open").clicked() {
                        chosen = Some(format!("open #{}", item.id));
                        menu_ui.close();
                    }
                } else if menu_ui.button("Close").clicked() {
                    chosen = Some(format!("close #{}", item.id));
                    menu_ui.close();
                }
            }
        });
        if let Some(cmd) = chosen {
            if clicked.is_none() {
                *clicked = click(ui, response, cmd);
            }
        }
    }
}
