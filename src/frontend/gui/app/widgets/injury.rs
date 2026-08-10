//! Injury doll and indicator widgets: severity palettes, doll art
//! resolution and rendering, the other-player injuries popup, and status
//! indicator content.

use super::*;

impl VellumGuiApp {
    /// ProfanityFE injury palette: none, injury 1-3, scar 1-3.
    /// The default severity palette as egui colors, parsed once from the
    /// shared `DEFAULT_INJURY_PALETTE` (single source of truth with the TUI and
    /// web). Used when no per-widget config override applies (e.g. the
    /// other-player injuries popup).
    pub(super) fn default_injury_palette() -> [Color32; 7] {
        std::array::from_fn(|i| {
            parse_hex_color(crate::config::DEFAULT_INJURY_PALETTE[i])
                .unwrap_or(Color32::GRAY)
        })
    }

    /// Resolve an injury doll's palette from its config (per-level overrides
    /// over the shared defaults) into egui colors. The GUI previously ignored
    /// these config fields entirely, so a user's injury*_color/scar*_color
    /// changed the TUI but not the GUI.
    pub(super) fn resolved_injury_palette(
        data: &crate::config::InjuryDollWidgetData,
    ) -> [Color32; 7] {
        let hex = data.resolved_colors();
        std::array::from_fn(|i| {
            parse_hex_color(&hex[i]).unwrap_or_else(|| Self::default_injury_palette()[i])
        })
    }

    /// Look up the fill color for a severity level in a resolved palette.
    pub(super) fn injury_level_color(palette: &[Color32; 7], level: u8) -> Color32 {
        palette[level.min(6) as usize]
    }

    /// Human-readable body part name for a hover tooltip ("leftArm" ->
    /// "left arm"); unknown protocol keys pass through unchanged.
    pub(super) fn doll_part_display_name(part: &str) -> &str {
        crate::config::skins::DOLL_PARTS
            .iter()
            .find(|(key, _, _)| key.eq_ignore_ascii_case(part))
            .map(|(_, display, _)| *display)
            .unwrap_or(part)
    }

    /// Human-readable severity for a hover tooltip.
    pub(super) fn injury_severity_text(level: u8) -> &'static str {
        match level.min(6) {
            0 => "uninjured",
            1 => "minor injury",
            2 => "moderate injury",
            3 => "severe injury",
            4 => "minor scar",
            5 => "moderate scar",
            _ => "severe scar",
        }
    }

    /// The CHARACTER'S OWN doll render state this frame: the active
    /// variant (first whose condition matches, in declaration order) and
    /// the active set's suppressed parts (each `hidden_when` evaluated).
    /// Another player's doll must pass `None` and an empty set to
    /// `render_injury_doll` instead — these conditions read self state,
    /// so your prone flag must never swap or hide someone else's doll.
    pub(super) fn resolve_doll_render(
        app_core: &AppCore,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
    ) -> (Option<usize>, std::collections::HashSet<String>) {
        let Some(art) = skin_art else {
            return (None, Default::default());
        };
        let now_server =
            chrono::Utc::now().timestamp() + app_core.message_processor.server_time_offset;
        let gameobj = app_core.gameobj_data_cached();
        let variant = art.resolve_doll_variant(&app_core.game_state, now_server, gameobj);
        let hidden = art
            .doll_set(variant)
            .hidden_parts(&app_core.game_state, now_server, gameobj);
        (variant, hidden)
    }

    /// Wrayth-style paperdoll drawn with painter geometry: each body part is
    /// a shape filled by its injury color, with a hover tooltip naming the
    /// part and severity. Back and nervous system have no spot on a front
    /// silhouette, so they render as "B"/"N" letters in the bottom corners
    /// (Wrayth-style). Scales with the window and needs no image assets.
    pub(super) fn render_injury_doll(
        ui: &mut egui::Ui,
        injuries: &HashMap<String, u8>,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
        doll_variant: Option<usize>,
        doll_hidden: &std::collections::HashSet<String>,
        grayscale: bool,
        palette: &[Color32; 7],
    ) {
        // Sprite mode: the active doll set's base body (default
        // `[injury_doll]`, or a condition-matched variant's set), then per
        // part either a hand-drawn state overlay (authored on the base's
        // canvas so it stacks in place) or a generated dot at the part's
        // calibrated anchor point.
        let set = skin_art.map(|art| art.doll_set(doll_variant));
        if let Some(set) = set.filter(|set| set.base.is_some()) {
            let base = set.base.unwrap();
            // Grayscale twins exist only while the checkbox demands them;
            // the generated dots keep their colors regardless.
            let base = if grayscale {
                set.base_gray.unwrap_or(base)
            } else {
                base
            };
            let avail = ui.available_size();
            let (outer, response) = ui.allocate_exact_size(
                Vec2::new(avail.x.max(40.0), avail.y.max(60.0)),
                egui::Sense::hover(),
            );
            let painter = ui.painter().with_clip_rect(outer);
            let dest = crate::frontend::gui::skin::sprite_dest(&base, outer);
            crate::frontend::gui::skin::paint_sprite(&painter, dest, &base, Color32::WHITE);
            let dot_radius = (set.dots.diameter * dest.height() / 2.0).max(4.0);
            let mut wounds: Vec<String> = Vec::new();
            // Walk the canonical part list, not the injuries map: fully
            // healthy parts are absent from the map entirely, and a part
            // with overlay art needs its level-0 (healthy) layer drawn.
            //
            // Z-ORDER: draw the nervous-system overlay FIRST (right above the
            // base), so any per-part wound/scar overlays and dots layer OVER
            // it — the nerves are a full-body underlay, not a badge that should
            // cover a limb wound. `DOLL_PARTS` lists nsys last (topmost), so
            // draw nsys, then everything else. (Tooltips are sorted later, so
            // this reordering doesn't affect them.)
            let ordered_parts = crate::config::skins::DOLL_PARTS
                .iter()
                .filter(|(part, _, _)| *part == "nsys")
                .chain(
                    crate::config::skins::DOLL_PARTS
                        .iter()
                        .filter(|(part, _, _)| *part != "nsys"),
                );
            for (part, _, _) in ordered_parts {
                let level = injuries.get(*part).copied().unwrap_or(0);
                // A suppressed part (hidden_when holds — e.g. a hand under
                // a severed arm) draws nothing at all; the wound still
                // lists in the tooltip below so the information survives.
                let suppressed = doll_hidden.contains(&part.to_ascii_lowercase());
                if suppressed {
                    // fall through to the tooltip push only
                } else if set.has_overlays(part) {
                    // Fully hand-drawn part: draw the current state's art
                    // if it exists; a state with no art lets the base show
                    // through — never a generated dot stamped on top of
                    // hand-drawn art. (Inverted skins author the base as
                    // the worst case, so "no overlay" is a deliberate
                    // reveal, e.g. a severed limb's hole.)
                    let overlay = if grayscale {
                        set.overlay_gray(part, level)
                            .or_else(|| set.overlay(part, level))
                    } else {
                        set.overlay(part, level)
                    };
                    if let Some(overlay) = overlay {
                        crate::frontend::gui::skin::paint_sprite(
                            &painter,
                            dest,
                            &overlay,
                            Color32::WHITE,
                        );
                    }
                } else if level > 0 {
                    // Artless part: the generated severity dot, as always.
                    let anchor = set.anchor(part);
                    let center = dest.min
                        + Vec2::new(anchor.x * dest.width(), anchor.y * dest.height());
                    crate::frontend::gui::skin::paint_severity_dot(
                        &painter,
                        center,
                        dot_radius,
                        level,
                        &set.dots,
                    );
                }
                if level > 0 {
                    wounds.push(format!(
                        "{}: {}",
                        Self::doll_part_display_name(part),
                        Self::injury_severity_text(level)
                    ));
                }
            }
            // No "uninjured" tooltip on an unwounded doll (it read as a stray
            // badge over the UberBar paperdoll); only surface actual wounds.
            if !wounds.is_empty() {
                wounds.sort();
                response.on_hover_text(wounds.join("\n"));
            }
            return;
        }

        // (key, display name, shape) in unit coordinates: x/y are fractions
        // of the doll rect; radii and line widths are fractions of its
        // height. Head must precede eyes so the eyes paint on top.
        enum PartShape {
            Circle { c: (f32, f32), r: f32 },
            Block { min: (f32, f32), max: (f32, f32) },
            Line { a: (f32, f32), b: (f32, f32), w: f32 },
            Letter { c: (f32, f32), letter: &'static str },
        }
        use PartShape::*;
        // Back and nervous system have no spot on a front silhouette; like
        // Wrayth's paperdoll they render as "B" and "N" letters in the
        // bottom corners, colored by severity.
        const PARTS: &[(&str, &str, PartShape)] = &[
            ("head", "head", Circle { c: (0.50, 0.105), r: 0.085 }),
            ("leftEye", "left eye", Circle { c: (0.465, 0.09), r: 0.018 }),
            ("rightEye", "right eye", Circle { c: (0.535, 0.09), r: 0.018 }),
            ("neck", "neck", Block { min: (0.465, 0.19), max: (0.535, 0.235) }),
            ("chest", "chest", Block { min: (0.38, 0.235), max: (0.62, 0.41) }),
            ("abdomen", "abdomen", Block { min: (0.395, 0.41), max: (0.605, 0.525) }),
            ("leftArm", "left arm", Line { a: (0.365, 0.26), b: (0.265, 0.47), w: 0.045 }),
            ("rightArm", "right arm", Line { a: (0.635, 0.26), b: (0.735, 0.47), w: 0.045 }),
            ("leftHand", "left hand", Circle { c: (0.25, 0.515), r: 0.033 }),
            ("rightHand", "right hand", Circle { c: (0.75, 0.515), r: 0.033 }),
            ("leftLeg", "left leg", Line { a: (0.44, 0.53), b: (0.41, 0.90), w: 0.055 }),
            ("rightLeg", "right leg", Line { a: (0.56, 0.53), b: (0.59, 0.90), w: 0.055 }),
            ("back", "back", Letter { c: (0.12, 0.93), letter: "B" }),
            ("nsys", "nervous system", Letter { c: (0.88, 0.93), letter: "N" }),
        ];

        // Fit an aspect-stable doll rect into the available space, centered
        // horizontally so narrow and wide windows both look intentional.
        const ASPECT: f32 = 0.75; // width : height
        let avail = ui.available_size();
        let mut height = avail.y.max(60.0);
        let mut width = height * ASPECT;
        if width > avail.x.max(40.0) {
            width = avail.x.max(40.0);
            height = width / ASPECT;
        }
        let (outer, _) =
            ui.allocate_exact_size(Vec2::new(avail.x.max(width), height), egui::Sense::hover());
        let rect = Rect::from_center_size(outer.center(), Vec2::new(width, height));
        let painter = ui.painter().with_clip_rect(outer);
        let at = |x: f32, y: f32| rect.min + Vec2::new(x * rect.width(), y * rect.height());
        let scale = rect.height();

        let letter_font = egui::FontId::proportional((scale * 0.09).clamp(10.0, 18.0));
        for (key, display, shape) in PARTS {
            let level = injuries.get(*key).copied().unwrap_or(0);
            let fill = Self::injury_level_color(palette, level);
            let outline = egui::Stroke::new(1.0, Self::lighten(fill, 0.2));

            let hover_rect = match shape {
                Circle { c, r } => {
                    let center = at(c.0, c.1);
                    painter.circle(center, r * scale, fill, outline);
                    Rect::from_center_size(center, Vec2::splat(r * scale * 2.0))
                }
                Block { min, max } => {
                    let shape_rect = Rect::from_min_max(at(min.0, min.1), at(max.0, max.1));
                    painter.rect(shape_rect, scale * 0.02, fill, outline, egui::StrokeKind::Middle);
                    shape_rect
                }
                Line { a, b, w } => {
                    let (a, b) = (at(a.0, a.1), at(b.0, b.1));
                    painter.line_segment([a, b], egui::Stroke::new(w * scale, fill));
                    Rect::from_two_pos(a, b).expand(w * scale * 0.5)
                }
                Letter { c, letter } => {
                    let center = at(c.0, c.1);
                    painter.text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        *letter,
                        letter_font.clone(),
                        fill,
                    );
                    Rect::from_center_size(center, Vec2::splat(scale * 0.11))
                }
            };

            ui.interact(
                hover_rect,
                ui.id().with(("injury_doll", key)),
                egui::Sense::hover(),
            )
            .on_hover_text(format!("{}: {}", display, Self::injury_severity_text(level)));
        }
    }

    /// Popup for viewing another player's injuries (server `injuries-*` dialog).
    pub(in crate::frontend::gui::app) fn render_injuries_popup(&mut self, ctx: &egui::Context) {
        let Some(popup) = self.app_core.ui_state.injuries_popup.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new(format!("{}'s Injuries", popup.player_name))
            .id(egui::Id::new("gui_injuries_popup"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.allocate_ui(Vec2::new(170.0, 225.0), |ui| {
                    // Another player's injuries: no per-widget config, so the
                    // shared default palette. Variants and hidden parts stay
                    // off — their conditions read SELF state, and your prone
                    // flag must not swap or hide someone else's doll.
                    Self::render_injury_doll(
                        ui,
                        &popup.injuries,
                        self.skin_state.widget_art().as_deref(),
                        None,
                        &Default::default(),
                        self.ui_settings.doll_grayscale,
                        &Self::default_injury_palette(),
                    );
                });
            });
        if !open {
            self.app_core.ui_state.injuries_popup = None;
        }
    }

    pub(super) fn render_indicator_content(
        ui: &mut egui::Ui,
        label: &str,
        indicator: &crate::data::IndicatorData,
        skin_art: Option<&crate::frontend::gui::skin::SkinWidgetArt>,
        gray_inactive: bool,
        resolved: &crate::core::conditions::ResolvedStatusArt,
    ) {
        let text = if label.is_empty() {
            &indicator.indicator_id
        } else {
            label
        };
        // A matched state's color wins; then the per-window color; then the
        // TUI defaults (#00ff00 active, #555555 off).
        let color = resolved
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| {
                if indicator.active {
                    indicator
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(Color32::from_rgb(0x00, 0xff, 0x00))
                } else {
                    Color32::from_rgb(0x55, 0x55, 0x55)
                }
            });
        // Icon precedence: a resolved IconRef (state icon or template active
        // icon) via the skin/pool, then — only when ACTIVE — the id-keyed skin
        // sprite and the built-in pictogram; custom ids without art keep the
        // text. When INACTIVE with no configured inactive icon (resolved.icon
        // is None), render NOTHING: inactive art is opt-in, never a dimmed
        // copy or a fallback pictogram. "Gray when inactive" still applies to
        // a configured inactive sprite.
        let inactive_blank = !indicator.active && resolved.icon.is_none();
        // Nothing to draw and no active pictogram to fall back to: leave the
        // cell blank (inactive with no configured inactive icon).
        if inactive_blank {
            return;
        }
        let mut grayed = false;
        let sprite = match &resolved.icon {
            Some(icon) => skin_art.and_then(|art| {
                if !indicator.active && gray_inactive {
                    // Grayscale twin of the configured inactive sprite, if any.
                    if let Some(gray) = art.icon_gray(&indicator.indicator_id) {
                        grayed = true;
                        return Some(gray);
                    }
                }
                art.resolve_icon_ref(icon, &indicator.indicator_id)
            }),
            // Active + no explicit icon: fall through to the id-keyed skin
            // sprite (and the built-in pictogram below) so "Default (by id)"
            // shows the built-in art.
            None => skin_art.and_then(|art| art.icon(&indicator.indicator_id)),
        };
        if sprite.is_some() || super::status_icons::supported(&indicator.indicator_id) {
            let side = ui
                .available_width()
                .min(ui.available_height())
                .clamp(10.0, 96.0);
            ui.centered_and_justified(|ui| {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
                if let Some(sprite) = sprite {
                    // Sprites carry their own colors: full-color when
                    // active, dimmed toward gray when inactive (or the
                    // full-strength gray twin when that setting is on).
                    let tint = if indicator.active || grayed {
                        Color32::WHITE
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 70)
                    };
                    let dest = crate::frontend::gui::skin::icon_dest(&sprite, rect);
                    crate::frontend::gui::skin::paint_icon(ui.painter(), dest, &sprite, tint);
                } else {
                    super::status_icons::paint(
                        ui.painter(),
                        rect,
                        &indicator.indicator_id,
                        color,
                        ui.visuals().window_fill(),
                    );
                }
                response.on_hover_text(text.to_string());
            });
            return;
        }
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(text).color(color).strong());
        });
    }

}
