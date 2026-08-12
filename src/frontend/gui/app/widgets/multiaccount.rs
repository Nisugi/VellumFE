//! Multi-account cards: one per sibling VellumFE instance on this machine,
//! drawn so that grouped characters read as grouped at a glance.
//!
//! The point is to answer "who is in trouble, and who is with whom" without
//! alt-tabbing between six clients. Groups get a shared frame with the leader
//! first; solo characters stand alone. A roster we are not sure about says so
//! rather than presenting a guess as fact.

use super::*;

use crate::core::multiaccount::{cluster_peers, Cluster, Freshness, PeerStatus};

/// Single-character glyphs for the conditions worth seeing across six cards
/// at once. Borrowed from Lich's groupbar, which packs status into one
/// colored row rather than a wide badge per condition -- with this many
/// cards, width is the scarce resource.
const STATUS_GLYPHS: &[(&str, &str, Color32)] = &[
    ("dead", "X", Color32::from_rgb(0xFF, 0x44, 0x44)),
    ("stunned", "S", Color32::from_rgb(0xFF, 0xD7, 0x00)),
    ("bleeding", "!", Color32::from_rgb(0xDC, 0x14, 0x3C)),
    ("webbed", "W", Color32::from_rgb(0xC0, 0xC0, 0xC0)),
    ("prone", "P", Color32::from_rgb(0xFF, 0xA5, 0x00)),
    ("kneeling", "K", Color32::from_rgb(0xFF, 0xA5, 0x00)),
    ("sitting", "s", Color32::from_rgb(0xFF, 0xA5, 0x00)),
    ("hidden", "H", Color32::from_rgb(0x69, 0x69, 0x69)),
    ("invisible", "i", Color32::from_rgb(0xAD, 0xD8, 0xE6)),
    ("poisoned", "T", Color32::from_rgb(0x32, 0xCD, 0x32)),
    ("diseased", "D", Color32::from_rgb(0x8B, 0x45, 0x13)),
];

/// Health band colors. Only health is banded -- the other vitals read as
/// "how much is left", but health reads as "how much trouble".
fn health_color(percent: u8) -> Color32 {
    match percent {
        0..=33 => Color32::from_rgb(0xFF, 0x44, 0x44),
        34..=66 => Color32::from_rgb(0xFF, 0xB0, 0x00),
        _ => Color32::from_rgb(0x2E, 0x7D, 0x32),
    }
}

impl VellumGuiApp {
    pub(super) fn render_multiaccount_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        data: &crate::config::MultiAccountWidgetData,
    ) {
        let now_ms = crate::core::multiaccount::hub::now_ms();

        // Our own status never arrives over a socket -- the hub deliberately
        // does not dial itself -- so the self card is built from game state
        // here. That also makes it work while the sidecar is still binding.
        let mut peers = (*settings.multiaccount_peers).clone();
        if data.show_self {
            peers.insert(
                crate::core::multiaccount::SELF_PORT,
                PeerStatus::from_local_named(
                    &app_core.game_state,
                    app_core
                        .config
                        .connection
                        .character
                        .as_deref()
                        .or(app_core.config.character.as_deref()),
                    now_ms,
                ),
            );
        }
        let peers = &peers;

        if peers.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("No other VellumFE sessions on this machine.")
                        .weak()
                        .italics(),
                );
                ui.label(
                    RichText::new("Cards appear automatically as you connect more clients.")
                        .weak()
                        .small(),
                );
            });
            return;
        }

        // Our own clock, for interpolating each peer's roundtime. Peers ship
        // absolute end stamps, so nothing streams a countdown.
        let now_server = app_core.game_state.game_time;
        let my_room = app_core.game_state.room_id.clone();

        let clusters = cluster_peers(peers);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    // Keep clusters on one flowing row; each one sizes to its
                    // own cards rather than claiming the remaining width.
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (idx, cluster) in clusters.iter().enumerate() {
                        Self::render_cluster(
                            ui,
                            settings,
                            data,
                            cluster,
                            idx,
                            peers,
                            now_ms,
                            now_server,
                            my_room.as_deref(),
                        );
                    }
                });
            });
    }

    /// One cluster: grouped characters share an enclosing frame, solo ones
    /// are drawn bare so the frame itself carries the "these are together"
    /// meaning.
    #[allow(clippy::too_many_arguments)]
    fn render_cluster(
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        data: &crate::config::MultiAccountWidgetData,
        cluster: &Cluster,
        cluster_idx: usize,
        peers: &std::collections::BTreeMap<u16, PeerStatus>,
        now_ms: u64,
        now_server: i64,
        my_room: Option<&str>,
    ) {
        let draw_cards = |ui: &mut egui::Ui| {
            // Header only where it carries information: who leads, and
            // whether we trust the roster.
            if !cluster.is_solo() {
                ui.horizontal(|ui| {
                    if let Some(name) = &cluster.leader_name {
                        let own = cluster.leader.is_some();
                        let label = if own {
                            format!("\u{2691} {name}")
                        } else {
                            // Following someone who is not one of ours.
                            format!("\u{2691} {name} (not yours)")
                        };
                        ui.label(RichText::new(label).strong().small());
                    }
                    if !cluster.confirmed {
                        ui.label(
                            RichText::new("roster unconfirmed")
                                .small()
                                .italics()
                                .color(Color32::from_rgb(0xFF, 0xB0, 0x00)),
                        )
                        .on_hover_text(
                            "One of these characters joined a group without seeing \
                             the full roster. Membership may be incomplete.",
                        );
                    }
                });
            }

            ui.horizontal_top(|ui| {
                for port in &cluster.members {
                    let Some(peer) = peers.get(port) else { continue };
                    Self::render_peer_card(
                        ui, settings, data, peer, now_ms, now_server, my_room,
                    );
                }
            });
        };

        // A cluster occupies exactly its cards' width. Without this the
        // enclosing layout hands it the full row and the next cluster is
        // pushed onto a new line, which is what made the cards stair-step.
        let card_span = |n: usize| data.card_width.max(80.0) * n as f32 + 28.0;

        if cluster.is_solo() {
            ui.allocate_ui(
                egui::vec2(card_span(cluster.members.len()), ui.available_height()),
                |ui| ui.vertical(draw_cards),
            );
        } else {
            ui.allocate_ui(
                egui::vec2(card_span(cluster.members.len()) + 12.0, ui.available_height()),
                |ui| {
                    let accent = widget_accent(ui.ctx(), ui.visuals());
                    egui::Frame::group(ui.style())
                        .stroke(egui::Stroke::new(1.5, accent))
                        .show(ui, |ui| {
                        ui.push_id(("multiaccount_cluster", cluster_idx), |ui| {
                            ui.vertical(draw_cards);
                        });
                    });
                },
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_peer_card(
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        data: &crate::config::MultiAccountWidgetData,
        peer: &PeerStatus,
        now_ms: u64,
        now_server: i64,
        my_room: Option<&str>,
    ) {
        let freshness = peer.freshness(now_ms);
        let width = data.card_width.max(80.0);

        // A different room is the "not with you" cue: it is the thing you
        // most want to notice without reading anything.
        let elsewhere = !peer.is_self()
            && match (my_room, peer.room_id.as_deref()) {
                (Some(mine), Some(theirs)) => mine != theirs,
                _ => false,
            };

        let mut frame = egui::Frame::group(ui.style());
        if peer.is_self() {
            // Gold marks "this is you" -- the reference point the other cards
            // are read against. Takes precedence over the different-room
            // stroke: you are never in a different room from yourself.
            frame = frame.stroke(egui::Stroke::new(
                2.0,
                Color32::from_rgb(0xFF, 0xD7, 0x00),
            ));
        } else if elsewhere {
            frame = frame.stroke(egui::Stroke::new(
                1.5,
                Color32::from_rgb(0xFF, 0x44, 0x44),
            ));
        }

        frame.show(ui, |ui| {
            ui.set_width(width);
            ui.push_id(("multiaccount_card", peer.port), |ui| {
                ui.vertical(|ui| {
                    // Stale data dims the whole card rather than hiding it:
                    // a peer mid-reconnect should look uncertain, not gone.
                    if freshness == Freshness::Stale {
                        ui.disable();
                    }

                    ui.horizontal(|ui| {
                        let name = RichText::new(&peer.character).strong();
                        let name = if peer.is_self() {
                            name.color(Color32::from_rgb(0xFF, 0xD7, 0x00))
                        } else {
                            name
                        };
                        ui.label(name);
                        if peer.is_self() {
                            // Not color alone: a marker that reads without it.
                            ui.label(RichText::new("(you)").weak().small());
                        }
                        if freshness == Freshness::Stale {
                            ui.label(
                                RichText::new("\u{25CF}")
                                    .small()
                                    .color(Color32::from_rgb(0xFF, 0xB0, 0x00)),
                            )
                            .on_hover_text(if peer.connected {
                                "No update recently"
                            } else {
                                "Disconnected \u{2014} last known values"
                            });
                        }
                    });

                    if data.show_status {
                        Self::render_status_glyphs(ui, peer);
                    }

                    if data.show_vitals {
                        Self::render_peer_vitals(ui, settings, peer);
                    }

                    if data.show_rt {
                        Self::render_peer_rt(ui, settings, peer, now_server);
                    }

                    if data.show_mind {
                        Self::render_peer_gauge(
                            ui,
                            settings,
                            "Mind",
                            peer.mind.as_ref(),
                            Color32::from_rgb(0x7C, 0xCD, 0x7C),
                        );
                    }
                    if data.show_stance {
                        Self::render_peer_gauge(
                            ui,
                            settings,
                            "Stance",
                            peer.stance.as_ref(),
                            Color32::from_rgb(0x5C, 0xAC, 0xEE),
                        );
                    }
                    if data.show_encumbrance {
                        Self::render_peer_gauge(
                            ui,
                            settings,
                            "Enc",
                            peer.encumbrance.as_ref(),
                            Color32::from_rgb(0xC4, 0xA0, 0x00),
                        );
                    }

                    if data.show_injuries {
                        // Peers ship wounds, not art, so the doll is drawn
                        // with OUR installed art. Variant stays None: variant
                        // rules resolve against the local character's
                        // conditions, which would be wrong for a peer.
                        //
                        // Height-capped so the doll sits inside the card
                        // rather than consuming the window -- it sizes to the
                        // space it is given.
                        let doll_h = (data.card_width * 0.9).clamp(60.0, 160.0);
                        ui.allocate_ui(egui::vec2(ui.available_width(), doll_h), |ui| {
                            Self::render_injury_doll(
                                ui,
                                &peer.injuries,
                                settings.skin_art.as_deref(),
                                None,
                                &Default::default(),
                                false,
                                &Self::default_injury_palette(),
                            );
                        });
                    }

                    if data.show_room {
                        if let Some(room) = &peer.room_name {
                            let text = RichText::new(room).small();
                            let text = if elsewhere {
                                text.color(Color32::from_rgb(0xFF, 0x88, 0x88))
                            } else {
                                text.weak()
                            };
                            ui.label(text).on_hover_text(if elsewhere {
                                "In a different room from you"
                            } else {
                                "Here with you"
                            });
                        }
                    }
                });
            });
        });
    }

    fn render_status_glyphs(ui: &mut egui::Ui, peer: &PeerStatus) {
        let active: Vec<_> = STATUS_GLYPHS
            .iter()
            .filter(|(id, _, _)| peer.indicators.get(id))
            .collect();
        if active.is_empty() {
            // A placeholder keeps the card height stable as conditions come
            // and go, so a row of cards does not jitter.
            ui.label(RichText::new("\u{00B7}").weak().small());
            return;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for (id, glyph, color) in active {
                ui.label(RichText::new(*glyph).color(*color).strong().small())
                    .on_hover_text(*id);
            }
        });
    }

    fn render_peer_vitals(
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        peer: &PeerStatus,
    ) {
        let v = &peer.vitals;
        let bars = [
            ("hp", v.health, health_color(v.health)),
            ("mp", v.mana, Color32::from_rgb(0x4A, 0x90, 0xD9)),
            ("st", v.stamina, Color32::from_rgb(0x90, 0xEE, 0x90)),
            ("sp", v.spirit, Color32::from_rgb(0xDD, 0xA0, 0xDD)),
        ];
        for (label, percent, fill) in bars {
            let fraction = (percent as f32 / 100.0).clamp(0.0, 1.0);
            // Animation ids MUST be per-character: a shared id would make all
            // six cards animate as one bar.
            let id = format!("ma_{}_{}", peer.port, label);
            let fraction = Self::animated_fraction(ui, &id, fraction);
            let bar = Self::styled_progress_bar(
                ui,
                settings,
                fraction,
                fill,
                format!("{} {}%", label.to_uppercase(), percent),
            );
            let resp = ui.add_sized([ui.available_width().max(40.0), 12.0], bar);
            Self::overlay_progress_frame(ui, resp.rect, settings.skin_art.as_deref());
        }
    }

    fn render_peer_rt(
        ui: &mut egui::Ui,
        _settings: &WidgetRenderSettings,
        peer: &PeerStatus,
        now_server: i64,
    ) {
        let rt = peer.roundtime_remaining(now_server);
        let ct = peer.casttime_remaining(now_server);
        if rt <= 0.0 && ct <= 0.0 {
            ui.label(RichText::new(" ").small());
            return;
        }
        ui.horizontal(|ui| {
            if rt > 0.0 {
                ui.label(
                    RichText::new(format!("RT {rt:.0}"))
                        .small()
                        .strong()
                        .color(Color32::from_rgb(0xFF, 0x66, 0x00)),
                );
            }
            if ct > 0.0 {
                ui.label(
                    RichText::new(format!("CT {ct:.0}"))
                        .small()
                        .strong()
                        .color(Color32::from_rgb(0x66, 0xAA, 0xFF)),
                );
            }
        });
    }

    /// A gauge the peer has never reported renders as unknown rather than as
    /// zero -- "stance 0%" would read as fully offensive, which is a lie.
    fn render_peer_gauge(
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        label: &str,
        gauge: Option<&crate::core::multiaccount::Gauge>,
        fill: Color32,
    ) {
        let Some(g) = gauge else {
            ui.label(RichText::new(format!("{label} \u{2014}")).weak().small())
                .on_hover_text("Not reported by this character yet");
            return;
        };
        let fraction = (g.value as f32 / 100.0).clamp(0.0, 1.0);
        let text = if g.text.is_empty() {
            format!("{label} {}%", g.value)
        } else {
            format!("{} {}", label, g.text)
        };
        let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
        let resp = ui.add_sized([ui.available_width().max(40.0), 12.0], bar);
        Self::overlay_progress_frame(ui, resp.rect, settings.skin_art.as_deref());
    }
}
