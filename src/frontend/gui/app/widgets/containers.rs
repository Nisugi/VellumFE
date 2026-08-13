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
            ui.weak("Requires direct mode (WRAYTH banner).");
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
        let heading_color = widget_accent(ui.ctx(), ui.visuals());

        egui::ScrollArea::vertical()
            .id_salt("containers_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut room_items_hidden = false;
                for (gi, group) in roots.iter().enumerate() {
                    if group.is_empty() {
                        continue;
                    }
                    // Groups 5/6 are room-relation: suppress when stale.
                    if gi >= 5 && room_stale {
                        room_items_hidden = true;
                        continue;
                    }
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(GROUPS[gi])
                            .strong()
                            .color(heading_color),
                    );
                    ui.separator();
                    for item in group {
                        Self::containers_node(
                            ui, item, &children, &weights, &counts, 0, &mut clicked, &mut click,
                        );
                    }
                }
                if room_items_hidden {
                    ui.add_space(6.0);
                    ui.weak("Room items omitted - snapshot is from another room (Refresh).");
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
            let weight = format!(" · {} lbs", Self::fmt_lbs(bd.total));
            let capacity = match item.in_capacity() {
                Some(cap) => format!(" · {} lbs cap", cap.pounds),
                None => String::new(),
            };
            let title = format!("{glyph}{} — {count}{weight}{capacity}", item.name);
            let header = egui::CollapsingHeader::new(title)
                .id_salt(("containers_node", item.id.as_str()))
                .default_open(false);
            let response = header
                .show(ui, |ui| {
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
                })
                .header_response
                .on_hover_text(format!(
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
