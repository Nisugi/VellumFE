//! The creature field: the room's hostiles as cards on a perspective
//! floor, placed by `core::creature_cards::solver`. P3 renders generated
//! placeholder cards (vector standees); sprite art replaces them when the
//! `[creature_card]` resolve cascade finds files (P4/P5).
//!
//! The solver plans on a fixed 880x470 virtual stage; this renderer maps
//! that stage into the widget rect uniformly (fit, centred), which is what
//! keeps every solver guarantee intact under any window size.

use super::*;
use crate::core::creature_cards::solver::{ScreenRect, Unit, STAGE_H, STAGE_W};
use egui::Stroke;

/// Virtual-stage -> widget-rect mapping.
struct StageMap {
    scale: f32,
    origin: egui::Pos2,
}

impl StageMap {
    fn fit(rect: egui::Rect) -> Self {
        let scale = (rect.width() / STAGE_W).min(rect.height() / STAGE_H);
        let w = STAGE_W * scale;
        let h = STAGE_H * scale;
        let origin = egui::pos2(
            rect.left() + (rect.width() - w) / 2.0,
            rect.top() + (rect.height() - h) / 2.0,
        );
        Self { scale, origin }
    }

    fn pt(&self, x: f32, y: f32) -> egui::Pos2 {
        egui::pos2(self.origin.x + x * self.scale, self.origin.y + y * self.scale)
    }

    fn rect(&self, r: &ScreenRect) -> egui::Rect {
        egui::Rect::from_min_max(self.pt(r.x0, r.y0), self.pt(r.x1, r.y1))
    }
}

/// Stable placeholder body color from the creature's noun, muted so status
/// glyphs and the reticule stay the loud elements.
fn body_color(noun: &str, dead: bool) -> Color32 {
    if dead {
        return Color32::from_gray(85);
    }
    let mut h: u32 = 2166136261;
    for b in noun.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    // Hue from the hash, fixed low saturation / mid value.
    let hue = (h % 360) as f32 / 360.0;
    let (r, g, b) = hsv_muted(hue);
    Color32::from_rgb(r, g, b)
}

fn hsv_muted(hue: f32) -> (u8, u8, u8) {
    let (s, v) = (0.35f32, 0.55f32);
    let i = (hue * 6.0).floor();
    let f = hue * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

impl VellumGuiApp {
    pub(super) fn render_creature_field_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        window_name: &str,
        settings: &WidgetRenderSettings,
    ) -> Option<GuiLinkClick> {
        // Per-window options from the layout def (shared with the TUI).
        let (show_grid, show_order) = match app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window_name)
        {
            Some(crate::config::WindowDef::CreatureField { data, .. }) => {
                (data.show_grid, data.show_order)
            }
            _ => (true, false),
        };

        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
        let painter = ui.painter_at(rect);
        let map = StageMap::fit(rect);
        let field = &app_core.creature_field;

        if field.units().is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No hostiles in the room",
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return None;
        }

        if show_grid {
            Self::paint_field_floor(&painter, &map, field);
        }

        let current_target =
            Self::normalize_entity_id(&app_core.game_state.target_list.current_target);
        let now_ms = ui.input(|i| i.time) * 1000.0;
        let now_server =
            chrono::Utc::now().timestamp() + app_core.message_processor.server_time_offset;
        let gameobj = app_core.gameobj_data_cached();
        let mut any_animated = false;

        // Skin creature art, prepared in the update loop; render only reads.
        let art_cache = settings
            .creature_art
            .as_ref()
            .map(|a| a.lock().expect("creature art lock"));
        let art_cache = art_cache.as_deref().filter(|c| c.active);

        // Far -> near (painter's algorithm), ground-z keyed in the solver.
        for &i in &field.draw_order() {
            let unit = &field.units()[i];
            for member in &unit.members {
                let Some(creature) = app_core
                    .game_state
                    .room_creatures
                    .iter()
                    .find(|c| &c.id == member)
                else {
                    continue;
                };
                let is_target = !current_target.is_empty()
                    && Self::normalize_entity_id(member) == current_target;
                any_animated |= Self::paint_creature_card(
                    &painter,
                    &map,
                    field,
                    unit,
                    creature,
                    is_target,
                    now_ms,
                    ui.visuals().dark_mode,
                    art_cache,
                    &app_core.game_state,
                    now_server,
                    gameobj,
                );
            }
        }

        if show_order {
            Self::paint_target_order(&painter, &map, field, &current_target);
        }

        // The stun swirl is the only motion; idle rooms request no frames.
        if any_animated {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }

        // Click-to-target: nearest card under the pointer wins.
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let mut order = field.draw_order();
                order.reverse(); // nearest first
                for &i in &order {
                    let unit = &field.units()[i];
                    let card = map.rect(&Self::lifted_rect(app_core, field, unit));
                    if card.contains(pos) {
                        // A mounted pair: pick the rider half above the
                        // saddle line, the mount below.
                        let member = if unit.members.len() > 1
                            && pos.y < card.top() + card.height() * 0.45
                        {
                            &unit.members[1]
                        } else {
                            &unit.members[0]
                        };
                        let id = Self::normalize_entity_id(member);
                        return Some(Self::gui_link_click_from_response(
                            &response,
                            ui,
                            Self::direct_command_link(format!("target #{id}")),
                        ));
                    }
                }
            }
        }
        None
    }

    /// The card rect with any airborne lift applied (screen-space, after
    /// projection — the floor footprint does not move).
    fn lifted_rect(
        app_core: &AppCore,
        field: &crate::core::creature_cards::solver::CreatureField,
        unit: &Unit,
    ) -> ScreenRect {
        let mut r = field.rect(unit);
        if let Some(lift) = Self::unit_lift(app_core, unit) {
            let dy = lift * (r.y1 - r.y0);
            r.y0 += dy;
            r.y1 += dy;
        }
        r
    }

    /// Airborne lift fraction for a unit (negative = up), from the lead
    /// member's flags. Manifest-driven LiftSpec values take over once
    /// creature-card skins are authored; these are the built-in defaults.
    fn unit_lift(app_core: &AppCore, unit: &Unit) -> Option<f32> {
        let creature = app_core
            .game_state
            .room_creatures
            .iter()
            .find(|c| unit.members.first() == Some(&c.id))?;
        let flags = creature.flags.as_ref()?;
        if flags.has_flag("flying") {
            Some(-0.22)
        } else if flags.has_flag("hovering") {
            Some(-0.12)
        } else {
            None
        }
    }

    fn paint_field_floor(
        painter: &egui::Painter,
        map: &StageMap,
        field: &crate::core::creature_cards::solver::CreatureField,
    ) {
        let grid = Stroke::new(1.0, Color32::from_white_alpha(14));
        let edge = Stroke::new(1.4, Color32::from_white_alpha(26));
        let cols = field.columns();
        let rows = field.params.rows;
        // Row lines (near/far edges heavier).
        for r in 0..=rows {
            let (a, b) = field.floor_row_line(r);
            let stroke = if r == 0 || r == rows { edge } else { grid };
            painter.line_segment([map.pt(a.0, a.1), map.pt(b.0, b.1)], stroke);
        }
        // Column lines.
        for k in 0..=cols.len() {
            let (a, b) = field.floor_col_line(k);
            let stroke = if k == 0 || k == cols.len() { edge } else { grid };
            painter.line_segment([map.pt(a.0, a.1), map.pt(b.0, b.1)], stroke);
        }
    }

    /// One creature's card: skin sprite art with manifest-driven overlays
    /// when the resolve cascade found a base, the generated placeholder
    /// standee otherwise. Returns true when it painted a moving effect
    /// (caller schedules a repaint).
    #[allow(clippy::too_many_arguments)]
    fn paint_creature_card(
        painter: &egui::Painter,
        map: &StageMap,
        field: &crate::core::creature_cards::solver::CreatureField,
        unit: &Unit,
        creature: &crate::core::state::Creature,
        is_target: bool,
        now_ms: f64,
        dark: bool,
        art_cache: Option<&crate::frontend::gui::skin::CreatureArtCache>,
        gs: &crate::core::state::GameState,
        now_server: i64,
        gameobj: Option<&crate::core::gameobj_data::GameObjData>,
    ) -> bool {
        let flags = creature.flags.as_ref();
        let dead = creature.is_dead();
        let noun = creature.noun.as_deref().unwrap_or("creature");

        // Skin path: base art + the manifest's resolved card (variant,
        // lift, overlays), all evaluated host-style through resolve_card.
        let art = art_cache.and_then(|c| c.base(noun, None));
        let resolved = match (art_cache, flags) {
            (Some(cache), Some(flags)) => Some(crate::core::creature_cards::resolve_card(
                &cache.card,
                flags,
                gs,
                now_server,
                gameobj,
            )),
            _ => None,
        };

        // Lift: the manifest's airborne variant wins; built-in defaults
        // cover skinless rooms.
        let lift = resolved
            .as_ref()
            .and_then(|r| r.lift())
            .map(|l| l.offset_y)
            .or_else(|| {
                flags
                    .filter(|f| f.has_flag("flying"))
                    .map(|_| -0.22f32)
                    .or_else(|| flags.filter(|f| f.has_flag("hovering")).map(|_| -0.12f32))
            });

        let base = field.rect(unit);
        let (foot_x, foot_y) = field.foot(unit);
        let mut r = base;
        if let Some(l) = lift {
            let dy = l * (r.y1 - r.y0);
            r.y0 += dy;
            r.y1 += dy;
        }
        let card = map.rect(&r);

        // Contact shadow stays at the floor footprint; softens with lift.
        let (shadow_scale, shadow_alpha) = resolved
            .as_ref()
            .and_then(|r| r.lift())
            .map(|l| (l.shadow_scale, (l.shadow_opacity * 60.0) as u8))
            .unwrap_or(if lift.is_some() { (0.55, 24) } else { (1.0, 60) });
        let shadow_w = card.width() * 0.55 * shadow_scale;
        painter.add(egui::epaint::PathShape::convex_polygon(
            ellipse_points(map.pt(foot_x, foot_y), shadow_w, shadow_w * 0.24),
            Color32::from_black_alpha(shadow_alpha),
            Stroke::NONE,
        ));

        let mut animated = false;
        let mut body = card;
        if let (Some(cache), Some(art)) = (art_cache, art) {
            // ---- sprite card -------------------------------------------
            // A matched variant with placeholder-free authored art replaces
            // the cascade's base; template paths keep the ground pose.
            let texture = resolved
                .as_ref()
                .and_then(|r| r.base_override())
                .filter(|p| !p.contains('{'))
                .and_then(|p| cache.overlays.get(p).cloned().flatten())
                .unwrap_or_else(|| art.texture.clone());
            // Aspect-fit into the card rect, anchored bottom-centre (the
            // foot point), like a standee on its base.
            let tex_size = texture.size_vec2();
            let scale = (card.width() / tex_size.x).min(card.height() / tex_size.y);
            let draw_w = tex_size.x * scale;
            let draw_h = tex_size.y * scale;
            let dest = egui::Rect::from_min_max(
                egui::pos2(card.center().x - draw_w / 2.0, card.bottom() - draw_h),
                egui::pos2(card.center().x + draw_w / 2.0, card.bottom()),
            );
            let tint = if dead {
                Color32::from_gray(110)
            } else {
                Color32::WHITE
            };
            painter.image(
                texture.id(),
                dest,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint,
            );
            body = dest;
            // Manifest overlays: quad layers warp/scale with the card,
            // screen layers sit flat above it. Ranked art resolves its
            // {severity} from the live derived-effect store.
            if let Some(resolved) = &resolved {
                animated |= Self::paint_card_overlays(
                    painter,
                    cache,
                    resolved,
                    art,
                    dest,
                    now_ms,
                    &|name| gs.creature_effect_severity(&creature.id, name),
                );
            }
        } else {
            // ---- placeholder standee -----------------------------------
            // Posture: prone/kneeling/sitting squash toward the ground
            // until real variant art exists.
            let downed = flags.is_some_and(|f| {
                f.has_flag("prone") || f.has_flag("kneeling") || f.has_flag("sitting")
            });
            if downed {
                body.set_top(body.top() + body.height() * 0.45);
            }
            let color = body_color(noun, dead);
            let head_r = body.width() * 0.22;
            let torso = egui::Rect::from_min_max(
                egui::pos2(body.left() + body.width() * 0.18, body.top() + head_r * 1.6),
                egui::pos2(body.right() - body.width() * 0.18, body.bottom()),
            );
            painter.rect_filled(torso, head_r * 0.8, color);
            painter.circle_filled(
                egui::pos2(body.center().x, body.top() + head_r),
                head_r,
                color,
            );
            // Boss brow: a heavier outline instead of a bigger palette.
            if flags.is_some_and(|f| f.is_boss()) {
                painter.rect_stroke(
                    torso,
                    head_r * 0.8,
                    Stroke::new(2.0, Color32::from_rgb(0xc9, 0xa2, 0x27)),
                    egui::StrokeKind::Outside,
                );
            }
        }
        if dead {
            let m = body.width() * 0.24;
            let stroke = Stroke::new(2.5, Color32::from_rgb(0xd2, 0x4b, 0x3c));
            painter.line_segment(
                [
                    egui::pos2(body.left() + m, body.top() + m * 0.6),
                    egui::pos2(body.right() - m, body.top() + m * 1.4),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(body.right() - m, body.top() + m * 0.6),
                    egui::pos2(body.left() + m, body.top() + m * 1.4),
                ],
                stroke,
            );
        }

        // Noun label at the feet.
        let label_color = if dark {
            Color32::from_white_alpha(150)
        } else {
            Color32::from_black_alpha(150)
        };
        painter.text(
            egui::pos2(card.center().x, card.bottom() + 2.0),
            egui::Align2::CENTER_TOP,
            noun,
            egui::FontId::proportional((11.0 * map.scale.max(0.6)).clamp(9.0, 13.0)),
            label_color,
        );

        // Status badges above the head (skip posture; it's drawn).
        let mut top_y = body.top();
        if let Some(f) = flags {
            let badges: Vec<&str> = f
                .statuses
                .iter()
                .map(String::as_str)
                .filter(|s| !matches!(*s, "prone" | "kneeling" | "sitting" | "flying" | "hovering" | "stunned"))
                .collect();
            if !badges.is_empty() {
                let text = badges.join(" ");
                painter.text(
                    egui::pos2(card.center().x, top_y - 4.0),
                    egui::Align2::CENTER_BOTTOM,
                    text,
                    egui::FontId::proportional(9.5),
                    Color32::from_rgb(0x4f, 0xd1, 0xc5),
                );
                top_y -= 14.0;
            }
        }

        // Stun swirl: three orbiting stars, wall-clock phased.
        if flags.is_some_and(|f| f.has_flag("stunned")) && !dead {
            animated = true;
            let orbit_r = (card.width() * 0.42).max(9.0);
            let cy = top_y - orbit_r * 0.42;
            for k in 0..3 {
                let ph = now_ms as f32 * 0.0026 + k as f32 * 2.0944;
                let sx = card.center().x + ph.cos() * orbit_r;
                let sy = cy + ph.sin() * orbit_r * 0.34;
                let depth = (ph.sin() + 1.0) / 2.0;
                let size = (card.width() * (0.10 + 0.05 * depth)).max(3.2);
                painter.circle_filled(
                    egui::pos2(sx, sy),
                    size * 0.5,
                    Color32::from_rgba_unmultiplied(0xf2, 0xc2, 0x3c, 140 + (depth * 100.0) as u8),
                );
            }
            top_y = cy - orbit_r * 0.34;
        }

        // Targeting reticule: downward green triangle, above everything.
        if is_target {
            let w = (card.width() * 0.34).max(11.0);
            let h = w * 0.86;
            let tip = egui::pos2(card.center().x, (top_y - w * 0.30).min(body.top()));
            painter.add(egui::epaint::PathShape::convex_polygon(
                vec![
                    tip,
                    egui::pos2(tip.x - w / 2.0, tip.y - h),
                    egui::pos2(tip.x + w / 2.0, tip.y - h),
                ],
                Color32::from_rgb(0x48, 0xc7, 0x74),
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(12, 40, 22, 217)),
            ));
        }
        animated
    }

    /// Manifest-driven overlay layers over one sprite card. Quad layers
    /// scale with the card (body-wrap = the art's alpha bbox, anchored =
    /// placed at an anchor fraction); screen layers sit flat above it and
    /// may animate (orbit / pulse from wall clock). Returns true when any
    /// active layer animates.
    fn paint_card_overlays(
        painter: &egui::Painter,
        cache: &crate::frontend::gui::skin::CreatureArtCache,
        resolved: &crate::core::creature_cards::ResolvedCard<'_>,
        art: &crate::frontend::gui::skin::CreatureArt,
        dest: egui::Rect,
        now_ms: f64,
        severity_of: &dyn Fn(&str) -> Option<u8>,
    ) -> bool {
        use crate::config::skins::{AnimateKind, OverlaySpace};
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let mut animated = false;
        // The art's alpha bbox mapped into the dest rect: what body-wrap
        // overlays cover, so canvas padding in the base costs nothing.
        let bbox = egui::Rect::from_min_max(
            egui::pos2(
                dest.left() + art.bbox[0] * dest.width(),
                dest.top() + art.bbox[1] * dest.height(),
            ),
            egui::pos2(
                dest.left() + art.bbox[2] * dest.width(),
                dest.top() + art.bbox[3] * dest.height(),
            ),
        );
        let anchor_pt = |name: &str| -> egui::Pos2 {
            // Manifest calibration wins; the art's derived head/feet cover
            // the common case; centre is the never-crash fallback.
            let frac = resolved
                .anchor(name)
                .or(match name {
                    "head" => Some(art.head),
                    "feet" => Some(art.feet),
                    _ => None,
                })
                .unwrap_or([0.5, 0.5]);
            egui::pos2(
                dest.left() + frac[0] * dest.width(),
                dest.top() + frac[1] * dest.height(),
            )
        };
        for overlay in &resolved.overlays {
            let image = if overlay.image.contains("{severity}") {
                // Ranked art: severity comes from the live derived-effect
                // store, looked up by the effect name the overlay's own
                // condition tests. No live rank = nothing to draw.
                let mut ids = Vec::new();
                overlay.when.referenced_crtr_status_ids(&mut ids);
                let Some(sev) = ids.iter().find_map(|id| severity_of(id)) else {
                    continue;
                };
                overlay.image.replace("{severity}", &sev.to_string())
            } else if overlay.image.contains('{') {
                continue; // per-creature placeholders are refused at load
            } else {
                overlay.image.clone()
            };
            let Some(texture) = cache.overlays.get(&image).cloned().flatten() else {
                continue;
            };
            let anim = overlay.animate.as_ref();
            animated |= anim.is_some();
            match overlay.space {
                OverlaySpace::Quad => {
                    let rect = match overlay.anchor.as_deref() {
                        // Body-wrap: stretched over the sprite's alpha bbox.
                        None => bbox,
                        Some(name) => {
                            // Anchored: sized to half the card width,
                            // aspect-preserving, centred on the anchor.
                            let pt = anchor_pt(name);
                            let ts = texture.size_vec2();
                            let w = dest.width() * 0.5;
                            let h = w * ts.y / ts.x.max(1.0);
                            egui::Rect::from_center_size(pt, egui::vec2(w, h))
                        }
                    };
                    let alpha = match anim.map(|a| a.kind) {
                        Some(AnimateKind::Pulse) => {
                            let period = anim.map(|a| a.period_ms).unwrap_or(2400).max(1);
                            let ph = (now_ms / period as f64 * std::f64::consts::TAU).sin();
                            (170.0 + ph * 85.0) as u8
                        }
                        Some(AnimateKind::Flicker) => {
                            let period = anim.map(|a| a.period_ms).unwrap_or(2400).max(1) as f64;
                            if (now_ms / (period / 6.0)) as u64 % 3 == 0 {
                                120
                            } else {
                                255
                            }
                        }
                        _ => 255,
                    };
                    painter.image(
                        texture.id(),
                        rect,
                        uv,
                        Color32::from_white_alpha(alpha),
                    );
                }
                OverlaySpace::Screen => {
                    let pt = anchor_pt(overlay.anchor.as_deref().unwrap_or("head"));
                    match anim.map(|a| a.kind) {
                        Some(AnimateKind::Orbit) => {
                            let a = anim.expect("kind implies spec");
                            let rx = dest.width() * a.rx;
                            let ry = dest.width() * a.ry;
                            let period = a.period_ms.max(1) as f64;
                            for k in 0..a.count.max(1) {
                                let ph = (now_ms / period * std::f64::consts::TAU) as f32
                                    + k as f32 * std::f32::consts::TAU
                                        / a.count.max(1) as f32;
                                let depth = (ph.sin() + 1.0) / 2.0;
                                let size = dest.width() * (0.10 + 0.05 * depth);
                                let center = egui::pos2(
                                    pt.x + ph.cos() * rx,
                                    pt.y - ry * 1.5 + ph.sin() * ry,
                                );
                                painter.image(
                                    texture.id(),
                                    egui::Rect::from_center_size(
                                        center,
                                        egui::vec2(size, size),
                                    ),
                                    uv,
                                    Color32::from_white_alpha(140 + (depth * 100.0) as u8),
                                );
                            }
                        }
                        _ => {
                            // Static screen layer: sits just above the
                            // anchor, sized like an anchored quad layer.
                            let ts = texture.size_vec2();
                            let w = dest.width() * 0.4;
                            let h = w * ts.y / ts.x.max(1.0);
                            painter.image(
                                texture.id(),
                                egui::Rect::from_center_size(
                                    egui::pos2(pt.x, pt.y - h * 0.8),
                                    egui::vec2(w, h),
                                ),
                                uv,
                                Color32::WHITE,
                            );
                        }
                    }
                }
            }
        }
        animated
    }

    fn paint_target_order(
        painter: &egui::Painter,
        map: &StageMap,
        field: &crate::core::creature_cards::solver::CreatureField,
        current_target: &str,
    ) {
        let order = field.target_order();
        if order.is_empty() {
            return;
        }
        let y_bar = STAGE_H - 16.0;
        let teal = Color32::from_rgb(0x4f, 0xd1, 0xc5);
        for (k, &i) in order.iter().enumerate() {
            let unit = &field.units()[i];
            let (fx, _) = field.foot(unit);
            let is_sel = unit
                .members
                .iter()
                .any(|m| Self::normalize_entity_id(m) == current_target);
            painter.circle_filled(
                map.pt(fx, y_bar),
                if is_sel { 5.0 } else { 3.4 } * map.scale.max(0.5),
                if is_sel {
                    Color32::from_rgb(0xc9, 0xa2, 0x27)
                } else {
                    teal.gamma_multiply(0.85)
                },
            );
            painter.text(
                map.pt(fx, y_bar - 8.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{}", k + 1),
                egui::FontId::monospace(9.0),
                teal,
            );
        }
    }
}

/// Points approximating an ellipse (convex polygon for the shadow).
fn ellipse_points(center: egui::Pos2, rx: f32, ry: f32) -> Vec<egui::Pos2> {
    (0..24)
        .map(|i| {
            let a = i as f32 / 24.0 * std::f32::consts::TAU;
            egui::pos2(center.x + a.cos() * rx, center.y + a.sin() * ry)
        })
        .collect()
}
