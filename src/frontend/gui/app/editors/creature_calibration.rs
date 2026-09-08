//! Creature calibrator: pick any pool creature image, click to place its
//! anchors — the built-in grounding pair (feet/head) plus arbitrary named
//! anchors for overlay layers (mouth, back, doll parts for wounds) — set
//! the floor footprint ellipse, world size, and lift, and save it all to
//! the image's sidecar (embedded in the PNG too, so the file travels
//! calibrated). The same metadata the field renderer consumes.

use std::collections::HashMap;

use super::super::VellumGuiApp;
use super::CalibrationOutcome;
use crate::config::pool::{self, CreatureFootprint, CreatureSidecar};
use crate::frontend::gui::image_store;
use crate::frontend::gui::skin::{self as gui_skin, SkinTexture};
use eframe::egui;

/// Anchor names offered up front; any other name can be added freely.
const SUGGESTED_ANCHORS: &[&str] = &["feet", "head", "mouth", "back", "saddle"];

/// Wound-part anchors, offered as a second suggestion group: the field's
/// injury overlays look these up by doll-part name on the image's sidecar
/// (`art.anchor(part)`), so placing them here positions wound markers and
/// `{token}_{part}{rank}` art per creature.
fn wound_anchor_names() -> impl Iterator<Item = &'static str> {
    crate::config::INJURY_AREAS.iter().copied()
}

struct CreatureChoice {
    label: String,
    pool_path: String,
    abs_path: std::path::PathBuf,
}

pub(crate) struct CreatureCalibrationState {
    choices: Vec<CreatureChoice>,
    selected: Option<usize>,
    texture: Option<egui::TextureHandle>,
    /// Working anchors, lowercase name -> image fractions.
    anchors: HashMap<String, [f32; 2]>,
    /// The anchor the next canvas click places.
    selected_anchor: String,
    new_anchor_name: String,
    footprint_on: bool,
    rx: f32,
    ry_auto: bool,
    ry: f32,
    /// Authored footprint centre (image fractions), round-tripped from the
    /// sidecar. Only X affects rendering (the shadow lies on the ground
    /// line); the stored Y is preserved verbatim, never repurposed. None =
    /// feet-centred, the renderer's default.
    footprint_center: Option<[f32; 2]>,
    size_on: bool,
    size: f32,
    lift_on: bool,
    lift: f32,
    /// Overlay art (creatures/wounds, creatures/status): drawn width as a
    /// fraction of the wearing creature's drawn width.
    overlay_scale_on: bool,
    overlay_scale: f32,
    error: Option<String>,
}

impl CreatureCalibrationState {
    /// Build the picker state from the pool listing. `None` (with the
    /// explanation in the outcome) when the pool has no creature images.
    pub(crate) fn open() -> (Option<Self>, CalibrationOutcome) {
        let mut outcome = CalibrationOutcome::default();
        // Deep listing: variant folders (creatures/<noun>/<variant>/) are
        // below the generic scanner's depth.
        let choices: Vec<CreatureChoice> = pool::list_creature_images()
            .into_iter()
            .map(|image| CreatureChoice {
                label: image.display_label(),
                pool_path: image.pool_path.clone(),
                abs_path: image.abs_path.clone(),
            })
            .collect();
        if choices.is_empty() {
            outcome.messages.push(
                "No creature images in the pool (global/images/creatures/). Drop PNGs there \
                 or install some with .jinx, then calibrate."
                    .to_owned(),
            );
            return (None, outcome);
        }
        let selected = (choices.len() == 1).then_some(0);
        let mut state = CreatureCalibrationState {
            choices,
            selected,
            texture: None,
            anchors: HashMap::new(),
            selected_anchor: "feet".to_owned(),
            new_anchor_name: String::new(),
            footprint_on: false,
            rx: 0.35,
            ry_auto: true,
            ry: 0.35 * 0.24,
            footprint_center: None,
            size_on: false,
            size: 1.0,
            lift_on: false,
            lift: 0.1,
            overlay_scale_on: false,
            overlay_scale: 1.0,
            error: None,
        };
        if let Some(index) = selected {
            load_creature_choice(&mut state, index);
        }
        (Some(state), outcome)
    }

    /// Render the calibrator window for one frame. Sets `closed` in the
    /// outcome when the user dismissed the window.
    pub(crate) fn ui(&mut self, ctx: &egui::Context) -> CalibrationOutcome {
        let mut outcome = CalibrationOutcome::default();
        let state = self;
        state.ensure_texture(ctx);
        let mut open = true;
        let mut save_request = false;
        let mut load_request: Option<usize> = None;

        egui::Window::new("Creature Calibration")
            .id(egui::Id::new("gui_creature_calibration"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(640.0)
            .default_height(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    "Click the sprite to place the selected anchor. feet grounds the sprite \
                     on the field; named anchors position status/wound overlay layers.",
                );
                let selected_label = state
                    .selected
                    .map(|index| state.choices[index].label.clone())
                    .unwrap_or_else(|| "Pick a creature".to_owned());
                egui::ComboBox::from_label("Creature image")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        for (index, choice) in state.choices.iter().enumerate() {
                            if ui
                                .selectable_label(state.selected == Some(index), &choice.label)
                                .clicked()
                            {
                                load_request = Some(index);
                            }
                        }
                    });

                let (Some(texture), Some(_)) = (&state.texture, state.selected) else {
                    return;
                };
                let sprite = SkinTexture {
                    texture: texture.id(),
                    size: texture.size_vec2(),
                };
                ui.separator();

                ui.horizontal_top(|ui| {
                    // Anchor list: suggested + present + add-your-own.
                    ui.vertical(|ui| {
                        ui.set_width(160.0);
                        let mut names: Vec<String> =
                            SUGGESTED_ANCHORS.iter().map(|s| s.to_string()).collect();
                        for key in state.anchors.keys() {
                            if !names.iter().any(|n| n.eq_ignore_ascii_case(key))
                                && !wound_anchor_names().any(|w| w.eq_ignore_ascii_case(key))
                            {
                                names.push(key.clone());
                            }
                        }
                        names.sort();
                        let mut anchor_row = |ui: &mut egui::Ui,
                                              state: &mut CreatureCalibrationState,
                                              name: String| {
                            let placed = state.anchors.contains_key(&name);
                            let label = if placed {
                                format!("{name} \u{2022}")
                            } else {
                                name.clone()
                            };
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(state.selected_anchor == name, label)
                                    .clicked()
                                {
                                    state.selected_anchor = name.clone();
                                }
                                if placed
                                    && ui
                                        .small_button("\u{2715}")
                                        .on_hover_text("Remove this anchor")
                                        .clicked()
                                {
                                    state.anchors.remove(&name);
                                }
                            });
                        };
                        for name in names {
                            anchor_row(ui, state, name);
                        }
                        // Wound parts: the injury overlays look these up by
                        // doll-part name, collapsed so the common anchors
                        // stay one glance.
                        egui::CollapsingHeader::new("Wound parts")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.weak(
                                    "A part WITHOUT an anchor draws its wound art \
                                     full-canvas over the base (author both at the \
                                     same resolution for 1:1 alignment). An anchor \
                                     switches that part to a small sprite centred \
                                     on it.",
                                );
                                let any_wound_anchor = wound_anchor_names()
                                    .any(|name| state.anchors.contains_key(name));
                                if any_wound_anchor
                                    && ui
                                        .button("Clear all wound anchors")
                                        .on_hover_text(
                                            "Full-canvas wound art for every part",
                                        )
                                        .clicked()
                                {
                                    for name in wound_anchor_names() {
                                        state.anchors.remove(name);
                                    }
                                }
                                for name in wound_anchor_names() {
                                    anchor_row(ui, state, name.to_string());
                                }
                            });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let field = egui::TextEdit::singleline(&mut state.new_anchor_name)
                                .hint_text("new anchor")
                                .desired_width(90.0);
                            let submitted = ui.add(field).lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (ui.button("+").clicked() || submitted)
                                && !state.new_anchor_name.trim().is_empty()
                            {
                                state.selected_anchor =
                                    state.new_anchor_name.trim().to_ascii_lowercase();
                                state.new_anchor_name.clear();
                            }
                        });
                        if ui
                            .button("Remove anchor")
                            .on_hover_text("Drop the selected anchor's placement")
                            .clicked()
                        {
                            let key = state.selected_anchor.to_ascii_lowercase();
                            state.anchors.remove(&key);
                        }
                    });

                    // Sprite canvas: anchors, ground line, footprint.
                    const CONTROLS_HEIGHT: f32 = 150.0;
                    let avail = ui.available_size();
                    let canvas = egui::Vec2::new(
                        avail.x.max(160.0),
                        (avail.y - CONTROLS_HEIGHT).max(200.0),
                    );
                    let (rect, response) = ui.allocate_exact_size(canvas, egui::Sense::click());
                    let painter = ui.painter().with_clip_rect(rect);
                    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
                    let dest = gui_skin::sprite_dest(&sprite, rect);
                    gui_skin::paint_sprite(&painter, dest, &sprite, egui::Color32::WHITE);

                    let at = |anchor: [f32; 2]| {
                        dest.min
                            + egui::Vec2::new(anchor[0] * dest.width(), anchor[1] * dest.height())
                    };
                    let feet = state
                        .anchors
                        .get("feet")
                        .copied()
                        .unwrap_or([0.5, 0.95]);

                    // Ground line through the feet anchor.
                    let ground_y = at(feet).y;
                    // Shadow centre X: authored centre wins, else feet.
                    let shadow_cx = state
                        .footprint_center
                        .map(|[x, _]| at([x, 0.0]).x)
                        .unwrap_or_else(|| at(feet).x);
                    painter.line_segment(
                        [
                            egui::pos2(dest.min.x, ground_y),
                            egui::pos2(dest.max.x, ground_y),
                        ],
                        egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(120, 200, 120, 160),
                        ),
                    );
                    // Footprint ellipse on the ground line.
                    if state.footprint_on {
                        let rx = state.rx * dest.width();
                        let ry = if state.ry_auto {
                            state.rx * 0.24
                        } else {
                            state.ry
                        } * dest.width();
                        let center = egui::pos2(shadow_cx, ground_y);
                        paint_ellipse(
                            &painter,
                            center,
                            rx,
                            ry,
                            egui::Stroke::new(
                                1.5,
                                egui::Color32::from_rgba_unmultiplied(120, 200, 120, 200),
                            ),
                        );
                    }

                    // All anchors; the selected one cross-haired.
                    let highlight = ui.visuals().hyperlink_color;
                    for (name, anchor) in &state.anchors {
                        let pos = at(*anchor);
                        let selected = name.eq_ignore_ascii_case(&state.selected_anchor);
                        let color = if selected {
                            highlight
                        } else {
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 200)
                        };
                        painter.circle_stroke(pos, 4.0, egui::Stroke::new(1.5, color));
                        painter.text(
                            pos + egui::vec2(6.0, -6.0),
                            egui::Align2::LEFT_BOTTOM,
                            name,
                            egui::FontId::proportional(11.0),
                            color,
                        );
                    }
                    if let Some(anchor) = state
                        .anchors
                        .get(&state.selected_anchor.to_ascii_lowercase())
                    {
                        let center = at(*anchor);
                        let stroke = egui::Stroke::new(1.0, highlight);
                        painter.line_segment(
                            [
                                egui::pos2(dest.min.x, center.y),
                                egui::pos2(dest.max.x, center.y),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                egui::pos2(center.x, dest.min.y),
                                egui::pos2(center.x, dest.max.y),
                            ],
                            stroke,
                        );
                    }

                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            if dest.contains(pos) && dest.width() > 0.0 && dest.height() > 0.0 {
                                let normalized = [
                                    ((pos.x - dest.min.x) / dest.width()).clamp(0.0, 1.0),
                                    ((pos.y - dest.min.y) / dest.height()).clamp(0.0, 1.0),
                                ];
                                let key = state.selected_anchor.to_ascii_lowercase();
                                state.anchors.insert(key, normalized);
                            }
                        }
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.footprint_on, "Footprint").on_hover_text(
                        "Floor ellipse for the contact shadow, centered on the feet anchor. \
                         Off = the generic standee shadow.",
                    );
                    if state.footprint_on {
                        ui.add(
                            egui::Slider::new(&mut state.rx, 0.05..=0.8)
                                .text("width")
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        );
                        ui.checkbox(&mut state.ry_auto, "auto depth");
                        if !state.ry_auto {
                            ui.add(
                                egui::Slider::new(&mut state.ry, 0.02..=0.5)
                                    .text("depth")
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            );
                        }
                    }
                });
                if state.footprint_on {
                    ui.horizontal(|ui| {
                        match state.footprint_center.as_mut() {
                            Some(center) => {
                                ui.label("Center x");
                                ui.add(
                                    egui::Slider::new(&mut center[0], 0.0..=1.0)
                                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                                )
                                .on_hover_text(
                                    "Shadow ellipse centre, as a fraction of the image \
                                     width. Only X affects rendering; the shadow stays \
                                     on the ground line.",
                                );
                                if ui
                                    .button("Reset to feet")
                                    .on_hover_text(
                                        "Drop the custom centre — the shadow re-centres \
                                         on the feet anchor",
                                    )
                                    .clicked()
                                {
                                    state.footprint_center = None;
                                }
                            }
                            None => {
                                if ui
                                    .button("Offset center\u{2026}")
                                    .on_hover_text(
                                        "Author a shadow centre offset from the feet \
                                         anchor (wide or leaning poses)",
                                    )
                                    .clicked()
                                {
                                    let feet = state
                                        .anchors
                                        .get("feet")
                                        .copied()
                                        .unwrap_or([0.5, 0.95]);
                                    state.footprint_center = Some(feet);
                                }
                                ui.weak("centered on the feet anchor");
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.size_on, "World size").on_hover_text(
                        "This creature's height in world units, overriding the family \
                         default — keeps art from different sources in scale.",
                    );
                    if state.size_on {
                        ui.add(
                            egui::DragValue::new(&mut state.size)
                                .range(0.05..=20.0)
                                .speed(0.05),
                        );
                    }
                    ui.checkbox(&mut state.lift_on, "Lift").on_hover_text(
                        "Ground clearance for a neutral pose that floats (wisps, spectres), \
                         as a fraction of the sprite height.",
                    );
                    if state.lift_on {
                        ui.add(
                            egui::Slider::new(&mut state.lift, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.overlay_scale_on, "Overlay scale")
                        .on_hover_text(
                            "For shared overlay art (creatures/wounds, creatures/status): \
                             drawn width as a fraction of the wearing creature's drawn \
                             width — it scales with each creature's world size. Off = \
                             1.0 (same width as the creature).",
                        );
                    if state.overlay_scale_on {
                        ui.add(
                            egui::Slider::new(&mut state.overlay_scale, 0.05..=2.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        );
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Save")
                        .on_hover_text(
                            "Writes anchors, footprint, size and lift to the image's sidecar \
                             (and embeds them in the PNG) — the calibration travels with the file",
                        )
                        .clicked()
                    {
                        save_request = true;
                    }
                    if ui
                        .button("Reset")
                        .on_hover_text("Reload the last saved calibration")
                        .clicked()
                    {
                        if let Some(index) = state.selected {
                            load_request = Some(index);
                        }
                    }
                });
                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });

        if let Some(index) = load_request {
            state.selected = Some(index);
            state.error = None;
            load_creature_choice(state, index);
        }
        if save_request {
            if let Some(index) = state.selected {
                // Scenery calibration (exclusion edges + aspect) lives in
                // the same sidecar; carry it through untouched.
                let existing: CreatureSidecar =
                    pool::read_sidecar(&state.choices[index].abs_path).unwrap_or_default();
                let sidecar = state.to_sidecar(&existing);
                match pool::write_creature_sidecar(&state.choices[index].abs_path, &sidecar) {
                    Ok(()) => {
                        state.error = None;
                        // The per-noun creature cache re-resolves on reload.
                        outcome.reload_art = true;
                        outcome.messages.push(format!(
                            "Creature calibration saved for '{}'.",
                            state.choices[index].pool_path
                        ));
                    }
                    Err(err) => state.error = Some(format!("Failed to save: {}", err)),
                }
            }
        }

        outcome.closed = !open;
        outcome
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_creature_calibration(&mut self) {
        if self.creature_calibration.is_some() {
            self.raise_editor(egui::Id::new("gui_creature_calibration"));
            return;
        }
        let (state, outcome) = CreatureCalibrationState::open();
        for message in outcome.messages {
            self.app_core.add_system_message(&message);
        }
        if let Some(state) = state {
            self.creature_calibration = Some(state);
        }
    }

    pub(in super::super) fn render_creature_calibration(&mut self, ctx: &egui::Context) {
        let Some(state) = self.creature_calibration.as_mut() else {
            return;
        };
        let outcome = state.ui(ctx);
        if outcome.closed {
            self.creature_calibration = None;
        }
        for message in outcome.messages {
            self.app_core.add_system_message(&message);
        }
        if outcome.reload_art {
            self.skin_state.force_reload();
        }
    }
}

fn load_creature_choice(state: &mut CreatureCalibrationState, index: usize) {
    let choice = &state.choices[index];
    // The texture reloads lazily from render (`ensure_texture` needs ctx).
    state.texture = None;
    let sidecar: CreatureSidecar = pool::read_sidecar(&choice.abs_path).unwrap_or_default();
    state.apply_sidecar(&sidecar);
}

impl CreatureCalibrationState {
    /// Adopt a loaded sidecar into the editor state. Everything authored —
    /// including the footprint centre — is retained for round-tripping.
    fn apply_sidecar(&mut self, sidecar: &CreatureSidecar) {
        self.anchors = sidecar
            .anchors
            .iter()
            .map(|(name, anchor)| (name.to_ascii_lowercase(), *anchor))
            .collect();
        self.footprint_on = sidecar.footprint.is_some();
        self.footprint_center = None;
        if let Some(fp) = sidecar.footprint {
            self.rx = fp.rx;
            self.ry_auto = fp.ry.is_none();
            self.ry = fp.effective_ry();
            self.footprint_center = fp.center;
        }
        self.size_on = sidecar.size.is_some();
        if let Some(size) = sidecar.size {
            self.size = size;
        }
        self.lift_on = sidecar.lift.is_some();
        if let Some(lift) = sidecar.lift {
            self.lift = lift;
        }
        self.overlay_scale_on = sidecar.overlay_scale.is_some();
        if let Some(scale) = sidecar.overlay_scale {
            self.overlay_scale = scale;
        }
    }

    /// The sidecar this editor state saves. `existing` carries through the
    /// scenery-calibration fields (exclude/aspect) that live in the same
    /// file; the footprint centre round-trips unless the user explicitly
    /// reset it to the feet anchor.
    fn to_sidecar(&self, existing: &CreatureSidecar) -> CreatureSidecar {
        CreatureSidecar {
            kind: None, // the writer stamps it
            anchors: self.anchors.clone(),
            footprint: self.footprint_on.then(|| CreatureFootprint {
                rx: self.rx,
                ry: (!self.ry_auto).then_some(self.ry),
                center: self.footprint_center,
            }),
            size: self.size_on.then_some(self.size),
            lift: self.lift_on.then_some(self.lift),
            overlay_scale: self.overlay_scale_on.then_some(self.overlay_scale),
            exclude: existing.exclude,
            aspect: existing.aspect,
        }
    }
}

impl CreatureCalibrationState {
    fn ensure_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() {
            return;
        }
        let Some(index) = self.selected else {
            return;
        };
        let choice = &self.choices[index];
        self.texture = image_store::load_texture_file(
            ctx,
            &choice.abs_path,
            &format!("creature-cal:{}", choice.pool_path),
            "creature calibration",
        );
        if self.texture.is_none() {
            self.error = Some(format!("Cannot load {}", choice.pool_path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_state() -> CreatureCalibrationState {
        CreatureCalibrationState {
            choices: Vec::new(),
            selected: None,
            texture: None,
            anchors: HashMap::new(),
            selected_anchor: "feet".to_owned(),
            new_anchor_name: String::new(),
            footprint_on: false,
            rx: 0.35,
            ry_auto: true,
            ry: 0.35 * 0.24,
            footprint_center: None,
            size_on: false,
            size: 1.0,
            lift_on: false,
            lift: 0.1,
            overlay_scale_on: false,
            overlay_scale: 1.0,
            error: None,
        }
    }

    fn authored_sidecar() -> CreatureSidecar {
        let mut sidecar = CreatureSidecar {
            footprint: Some(CreatureFootprint {
                rx: 0.46,
                ry: Some(0.12),
                center: Some([0.3, 0.6]),
            }),
            ..Default::default()
        };
        sidecar.anchors.insert("feet".to_string(), [0.48, 0.9]);
        sidecar.anchors.insert("head".to_string(), [0.5, 0.1]);
        sidecar
    }

    #[test]
    fn footprint_center_roundtrips_through_unrelated_edits() {
        let mut state = blank_state();
        state.apply_sidecar(&authored_sidecar());
        assert_eq!(state.footprint_center, Some([0.3, 0.6]));
        // Change only the head anchor — the authored centre (X and the
        // stored, unrendered Y) must survive the save untouched.
        state.anchors.insert("head".to_string(), [0.55, 0.05]);
        let saved = state.to_sidecar(&CreatureSidecar::default());
        let fp = saved.footprint.unwrap();
        assert_eq!(fp.center, Some([0.3, 0.6]));
        assert_eq!(fp.rx, 0.46);
        assert_eq!(fp.ry, Some(0.12));
        assert_eq!(saved.anchors["head"], [0.55, 0.05]);
        // Explicit reset to the feet anchor drops the authored centre.
        state.footprint_center = None;
        let reset = state.to_sidecar(&CreatureSidecar::default());
        assert_eq!(reset.footprint.unwrap().center, None);
    }

    #[test]
    fn footprint_center_survives_external_and_png_embedded_output() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("coyote.png");
        // A real PNG so the writer can embed metadata in it.
        image::RgbaImage::new(2, 2).save(&image_path).unwrap();

        pool::write_creature_sidecar(&image_path, &authored_sidecar()).unwrap();

        // Load -> edit an anchor -> save, through the editor state.
        let mut state = blank_state();
        let loaded: CreatureSidecar = pool::read_sidecar(&image_path).unwrap();
        state.apply_sidecar(&loaded);
        state.anchors.insert("head".to_string(), [0.6, 0.08]);
        pool::write_creature_sidecar(&image_path, &state.to_sidecar(&loaded)).unwrap();

        // External sidecar keeps the authored centre.
        let external: CreatureSidecar = pool::read_sidecar(&image_path).unwrap();
        assert_eq!(external.footprint.unwrap().center, Some([0.3, 0.6]));
        assert_eq!(external.anchors["head"], [0.6, 0.08]);

        // PNG-embedded copy keeps it too: delete the sidecar file and let
        // the reader re-hydrate from the image.
        std::fs::remove_file(image_path.with_extension("toml")).unwrap();
        let embedded: CreatureSidecar = pool::read_sidecar(&image_path)
            .expect("embedded metadata should hydrate a sidecar");
        assert_eq!(embedded.footprint.unwrap().center, Some([0.3, 0.6]));
        assert_eq!(embedded.anchors["head"], [0.6, 0.08]);
    }
}

/// Stroke an axis-aligned ellipse (egui has no ellipse primitive).
fn paint_ellipse(
    painter: &egui::Painter,
    center: egui::Pos2,
    rx: f32,
    ry: f32,
    stroke: egui::Stroke,
) {
    const SEGMENTS: usize = 48;
    let points: Vec<egui::Pos2> = (0..=SEGMENTS)
        .map(|i| {
            let t = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            egui::pos2(center.x + rx * t.cos(), center.y + ry * t.sin())
        })
        .collect();
    painter.add(egui::Shape::line(points, stroke));
}
