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
    // Grouped. Worth showing even though the card also draws group frames:
    // the frame proves WE think they are grouped, this is the game saying so
    // -- and they disagree exactly when a roster is stale.
    ("joined", "J", Color32::from_rgb(0x00, 0xBF, 0xFF)),
];

/// Indicators the game sends that are deliberately not drawn.
///
/// STANDING is the normal state; a glyph on every card at all times carries
/// no information. The postures that DO matter (kneeling/sitting/prone) each
/// have their own glyph above.
const IGNORED_GLYPHS: &[&str] = &["standing"];

/// Compact duration: seconds under a minute, else m:ss. A card has no room
/// for "01:23:45" next to a spell name.
fn fmt_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

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

        // The snapshot -- peers plus the self card -- is assembled once per
        // frame in update(); this render path only borrows it. The one case
        // that still copies is show_self=false, which must subtract a card.
        let all = &*settings.multiaccount_peers;
        let filtered;
        let peers: &std::collections::BTreeMap<u16, PeerStatus> = if data.show_self {
            all
        } else {
            filtered = all
                .iter()
                .filter(|(port, _)| **port != crate::core::multiaccount::SELF_PORT)
                .map(|(port, peer)| (*port, peer.clone()))
                .collect();
            &filtered
        };

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

        // Server "now", for interpolating every peer's roundtime and effect
        // timers. Peers ship absolute end stamps, so nothing streams a
        // countdown -- but this must be a LIVE clock, not
        // `game_state.game_time`, which only advances when a prompt arrives.
        // Using the prompt clock froze every peer's RT between prompts.
        let now_server = chrono::Utc::now().timestamp() + app_core.server_time_offset;
        let my_room = app_core
            .nav_room_id
            .clone()
            .or_else(|| app_core.lich_room_id.clone())
            .or_else(|| app_core.game_state.room_id.clone());

        let mut clusters = cluster_peers(peers);
        // "group" keeps clustered characters adjacent, which is the whole
        // point of the frames; the other modes flatten that deliberately.
        match data.sort_by.as_str() {
            "name" => clusters.sort_by_key(|c| {
                c.members
                    .first()
                    .and_then(|p| peers.get(p))
                    .map(|p| p.character.to_ascii_lowercase())
                    .unwrap_or_default()
            }),
            "port" => clusters.sort_by_key(|c| c.members.first().copied().unwrap_or(0)),
            // "group" (default): cluster_peers already returns a stable order
            // with the self card first.
            _ => {}
        }

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
                    if cluster.leader_name.is_none() && cluster.grouped {
                        ui.label(
                            RichText::new("\u{2691} grouped")
                                .strong()
                                .small()
                                .color(Color32::from_rgb(0x00, 0xBF, 0xFF)),
                        )
                        .on_hover_text(
                            "The game reports these characters as grouped, but no \
                             roster has been parsed yet. Type `group` to confirm.",
                        );
                    }
                    if let Some(name) = &cluster.leader_name {
                        // "not yours" means the leader is not one of OUR
                        // characters -- decided by name, because
                        // `cluster.leader` is only set when that character's
                        // own roster says SelfLed. An unconfirmed roster left
                        // it None and mislabelled your own character.
                        let own = peers
                            .values()
                            .any(|p| p.character.eq_ignore_ascii_case(name));
                        let label = if own {
                            format!("\u{2691} {name}")
                        } else {
                            format!("\u{2691} {name} (not yours)")
                        };
                        ui.label(RichText::new(label).strong().small())
                            .on_hover_text(if own {
                                "Group leader"
                            } else {
                                "Leader is not one of your characters"
                            });
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
                        // Room id, right-aligned opposite the name -- bare
                        // number, no label; the room NAME rides the hover.
                        // Red is the "not with you" cue.
                        if data.show_room {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if let Some(id) = &peer.room_id {
                                        let text = RichText::new(id.as_str()).small();
                                        let text = if elsewhere {
                                            text.color(Color32::from_rgb(0xFF, 0x88, 0x88))
                                        } else {
                                            text.weak()
                                        };
                                        let hover = match (&peer.room_name, elsewhere) {
                                            (Some(name), true) => {
                                                format!("{name} \u{2014} not with you")
                                            }
                                            (Some(name), false) => {
                                                format!("{name} \u{2014} here with you")
                                            }
                                            (None, true) => {
                                                "In a different room from you".to_string()
                                            }
                                            (None, false) => "Here with you".to_string(),
                                        };
                                        ui.label(text).on_hover_text(hover);
                                    } else if peer.room_name.is_some() {
                                        // Name but no id (direct connect
                                        // without Lich): a marker beats a
                                        // blank corner; the name is on hover.
                                        ui.label(RichText::new("?").weak().small())
                                            .on_hover_text(format!(
                                                "{} \u{2014} room id unknown",
                                                peer.room_name.as_deref().unwrap_or("")
                                            ));
                                    }
                                },
                            );
                        }
                    });

                    // Rows are drawn in the configured order so a card can
                    // lead with whatever that user reads first. Anything not
                    // listed keeps its default position, so a partial or
                    // absent list still shows every enabled row.
                    // Rows are grouped into lines so short ones (RT, status
                    // icons) can share a line instead of each burning a full
                    // one. A single-row line draws exactly as before.
                    for line in data.row_lines() {
                        if line.len() == 1 {
                            Self::render_card_row(
                                ui,
                                settings,
                                data,
                                peer,
                                line[0].clone(),
                                now_server,
                                elsewhere,
                            );
                        } else {
                            // Split the width evenly, minus the spacing
                            // between items, so a bar sharing a line does not
                            // claim the whole row.
                            // Bars need an explicit share or the first one
                            // swallows the line; labels and icon strips size
                            // themselves and should not be padded out. Split
                            // the width only among the rows that stretch.
                            let stretchy = line
                                .iter()
                                .filter(|row| Self::row_stretches(row))
                                .count();
                            let count = line.len() as f32;
                            let gap = ui.spacing().item_spacing.x * (count - 1.0);
                            let share = if stretchy > 0 {
                                ((ui.available_width() - gap) / stretchy as f32).max(28.0)
                            } else {
                                0.0
                            };
                            ui.horizontal(|ui| {
                                for row in line {
                                    if Self::row_stretches(&row) {
                                        ui.allocate_ui(
                                            egui::vec2(share, ui.available_height()),
                                            |ui| {
                                                Self::render_card_row(
                                                    ui, settings, data, peer, row,
                                                    now_server, elsewhere,
                                                );
                                            },
                                        );
                                    } else {
                                        Self::render_card_row(
                                            ui, settings, data, peer, row, now_server,
                                            elsewhere,
                                        );
                                    }
                                }
                            });
                        }
                    }
                });
            });
        });
    }

    /// Whether a row fills the width it is given (a bar) or sizes to its own
    /// content (a label or icon strip). Only the former needs an explicit
    /// share when several rows sit on one line.
    fn row_stretches(row: &str) -> bool {
        matches!(
            row,
            "vitals" | "mind" | "stance" | "field_exp" | "encumbrance" | "injuries"
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_card_row(
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        data: &crate::config::MultiAccountWidgetData,
        peer: &PeerStatus,
        row: String,
        now_server: i64,
        elsewhere: bool,
    ) {
        match row.as_str() {
            "status" if data.show_status => Self::render_status_glyphs(ui, peer),
            "vitals" if data.show_vitals => {
                Self::render_peer_vitals(ui, settings, peer, data.show_absolute_vitals)
            }
            "rt" if data.show_rt => Self::render_peer_rt(ui, settings, peer, now_server),
            "hands" if data.show_hands => Self::render_peer_hands(ui, peer),
            "effects" if data.show_effects => {
                Self::render_peer_effects(ui, data, peer, now_server)
            }
            "mind" if data.show_mind => Self::render_peer_gauge(
                ui,
                settings,
                "Mind",
                peer.mind.as_ref(),
                Color32::from_rgb(0x7C, 0xCD, 0x7C),
            ),
            "stance" if data.show_stance => Self::render_peer_gauge(
                ui,
                settings,
                "Stance",
                peer.stance.as_ref(),
                Color32::from_rgb(0x5C, 0xAC, 0xEE),
            ),
            "field_exp" if data.show_field_exp => {
                Self::render_peer_field_exp(ui, settings, peer)
            }
            "encumbrance" if data.show_encumbrance => Self::render_peer_gauge(
                ui,
                settings,
                "Enc",
                peer.encumbrance.as_ref(),
                Color32::from_rgb(0xC4, 0xA0, 0x00),
            ),
            "injuries" if data.show_injuries => {
                // Peers ship wounds, not art, so the doll uses OUR installed
                // art. Variant stays None: variant rules resolve against the
                // local character's conditions, wrong for a peer.
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
            _ => {}
        }
    }


    /// Active conditions as pictograms, falling back to a letter for ids the
    /// icon set does not cover.
    ///
    /// Letters alone were near-unreadable at card size; `status_icons` is the
    /// same art the dedicated indicator widgets use, so a condition looks the
    /// same wherever it appears.
    fn render_status_glyphs(ui: &mut egui::Ui, peer: &PeerStatus) {
        let active: Vec<(&str, Color32)> = STATUS_GLYPHS
            .iter()
            .filter(|(id, _, _)| peer.indicators.get(id))
            .map(|(id, _, color)| (*id, *color))
            .collect();

        // Anything the game reports that has no glyph of its own. StatusInfo
        // is a general map, so a new indicator reaches us without a code
        // change -- this makes it visible without one too.
        let unknown: Vec<&str> = peer
            .indicators
            .iter()
            .filter(|(id, active)| {
                *active
                    && !IGNORED_GLYPHS.contains(id)
                    && !STATUS_GLYPHS.iter().any(|(known, _, _)| known == id)
            })
            .map(|(id, _)| id)
            .collect();

        if active.is_empty() && unknown.is_empty() {
            // A placeholder keeps the card height stable as conditions come
            // and go, so a row of cards does not jitter.
            ui.label(RichText::new("\u{00B7}").weak().small());
            return;
        }

        let size = 14.0;
        let bg = ui.visuals().panel_fill;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for (id, color) in active {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    // Fall back to a letter when the pictogram set has no art
                    // for this id, rather than leaving a blank gap.
                    if !crate::frontend::gui::app::status_icons::paint(
                        ui.painter(),
                        rect,
                        id,
                        color,
                        bg,
                    ) {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            id.chars().next().unwrap_or('?').to_uppercase().to_string(),
                            egui::FontId::proportional(size * 0.8),
                            color,
                        );
                    }
                }
                response.on_hover_text(
                    crate::frontend::gui::app::status_icons::display_name(id),
                );
            }
            for id in unknown.iter() {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        id.chars().next().unwrap_or('?').to_uppercase().to_string(),
                        egui::FontId::proportional(size * 0.8),
                        ui.visuals().text_color(),
                    );
                }
                response.on_hover_text(*id);
            }
        });
    }

    fn render_peer_vitals(
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        peer: &PeerStatus,
        absolute: bool,
    ) {
        let v = &peer.vitals;
        let bars = [
            ("hp", "health", v.health, health_color(v.health)),
            ("mp", "mana", v.mana, Color32::from_rgb(0x4A, 0x90, 0xD9)),
            ("st", "stamina", v.stamina, Color32::from_rgb(0x90, 0xEE, 0x90)),
            ("sp", "spirit", v.spirit, Color32::from_rgb(0xDD, 0xA0, 0xDD)),
        ];
        for (label, id, percent, fill) in bars {
            // Absolute numbers when the peer reported them; percentages are
            // always available, so they stay the fallback rather than
            // leaving a blank bar.
            let text = match (absolute, peer.minivitals.get(id)) {
                (true, Some((cur, max))) if *max > 0 => {
                    format!("{} {}/{}", label.to_uppercase(), cur, max)
                }
                _ => format!("{} {}%", label.to_uppercase(), percent),
            };
            let fraction = (percent as f32 / 100.0).clamp(0.0, 1.0);
            // Animation ids MUST be per-character: a shared id would make all
            // six cards animate as one bar.
            let id = format!("ma_{}_{}", peer.port, label);
            let fraction = Self::animated_fraction(ui, &id, fraction);
            let bar = Self::styled_progress_bar(ui, settings, fraction, fill, text);
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
            // A thin placeholder rather than nothing: the row keeps its slot
            // so the icons beside it do not jump left and back every time a
            // roundtime starts and ends.
            ui.add_space(2.0);
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

    fn render_peer_hands(ui: &mut egui::Ui, peer: &PeerStatus) {
        // Only draw what is there: two "empty" lines per card, times six
        // cards, is noise.
        for (slot, item) in [("L", &peer.left_hand), ("R", &peer.right_hand)] {
            if let Some(item) = item.as_deref().filter(|s| !s.is_empty()) {
                ui.label(RichText::new(format!("{slot} {item}")).small().weak())
                    .on_hover_text(item);
            }
        }
        if let Some(spell) = peer.prepared_spell.as_deref().filter(|s| !s.is_empty()) {
            ui.label(
                RichText::new(format!("\u{25C6} {spell}"))
                    .small()
                    .color(Color32::from_rgb(0x9A, 0x7C, 0xE0)),
            );
        }
    }

    /// Debuffs and cooldowns, filtered and capped.
    ///
    /// Six characters with unfiltered spell lists is unreadable, so this
    /// shows only the configured categories, applies the user's name filter,
    /// and caps the count -- surfacing "+N more" rather than silently
    /// truncating.
    fn render_peer_effects(
        ui: &mut egui::Ui,
        data: &crate::config::MultiAccountWidgetData,
        peer: &PeerStatus,
        now_server: i64,
    ) {
        let mut shown = 0usize;
        let mut hidden = 0usize;

        for category in &data.effect_categories {
            let Some(content) = peer.effects.get(category) else {
                continue;
            };
            for effect in &content.effects {
                // Expired entries linger until the game clears them.
                if let Some(end) = effect.expires_at {
                    if end <= now_server {
                        continue;
                    }
                }
                if !data.effect_filter.is_empty() {
                    let name = effect.text.to_ascii_lowercase();
                    if !data
                        .effect_filter
                        .iter()
                        .any(|needle| name.contains(&needle.to_ascii_lowercase()))
                    {
                        continue;
                    }
                }
                if shown >= data.max_effects {
                    hidden += 1;
                    continue;
                }
                shown += 1;

                let remaining = effect
                    .expires_at
                    .map(|end| (end - now_server).max(0))
                    .unwrap_or(0);
                let label = if remaining > 0 {
                    format!("{} {}", effect.text, fmt_duration(remaining))
                } else {
                    effect.text.clone()
                };
                // Cooldowns read as "not ready yet", debuffs as "in trouble":
                // different urgency, different color.
                let color = if category.eq_ignore_ascii_case("cooldowns") {
                    Color32::from_rgb(0x8A, 0x9B, 0xB0)
                } else {
                    Color32::from_rgb(0xE0, 0x7A, 0x5F)
                };
                ui.label(RichText::new(label).small().color(color))
                    .on_hover_text(format!("{category}: {}", effect.text));
            }
        }

        if hidden > 0 {
            ui.label(
                RichText::new(format!("+{hidden} more"))
                    .small()
                    .weak()
                    .italics(),
            )
            .on_hover_text("Raise the effect limit or narrow the filter in this window's menu");
        }
    }

    /// Unabsorbed field experience, colored by how close to the cap it is.
    /// Capped FXP is wasted FXP, so the bar turns urgent rather than just
    /// full -- the whole reason to watch it on someone else's card.
    fn render_peer_field_exp(
        ui: &mut egui::Ui,
        settings: &WidgetRenderSettings,
        peer: &PeerStatus,
    ) {
        let Some((value, max)) = peer.field_exp else {
            ui.label(RichText::new("FXP \u{2014}").weak().small())
                .on_hover_text("Not reported by this character yet");
            return;
        };
        let fraction = (value as f32 / max as f32).clamp(0.0, 1.0);
        let fill = if fraction >= 0.95 {
            Color32::from_rgb(0xFF, 0x44, 0x44)
        } else if fraction >= 0.75 {
            Color32::from_rgb(0xFF, 0xB0, 0x00)
        } else {
            Color32::from_rgb(0xFF, 0xD7, 0x00)
        };
        let bar = Self::styled_progress_bar(
            ui,
            settings,
            fraction,
            fill,
            format!("FXP {value}/{max}"),
        );
        let resp = ui.add_sized([ui.available_width().max(40.0), 12.0], bar);
        Self::overlay_progress_frame(ui, resp.rect, settings.skin_art.as_deref());
        if fraction >= 0.95 {
            resp.on_hover_text("At or near the field-experience cap \u{2014} time to absorb");
        }
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
