//! Creature calibrator: a ruler stage (feet up from a ground line, the
//! scale-spec's world) with the creature sprite standing on it. Fit mode:
//! pick the body type, a red dotted line marks the bestiary height with a
//! note saying which landmark belongs on it (crown, shoulder, diameter…),
//! and the user drags/sizes the sprite until it lines up — that fit IS
//! the calibration (world size + grounding), written to the image's
//! sidecar. Anchor mode locks the sprite and places named anchors (feet/
//! head/mouth + doll parts for wounds) by clicking. Footprint ellipse,
//! lift and overlay scale ride along as before. Sidecar embedded in the
//! PNG too, so the file travels calibrated.

use std::collections::HashMap;

use super::super::VellumGuiApp;
use super::CalibrationOutcome;
use crate::config::pool::{self, CreatureFootprint, CreatureSidecar};
use crate::core::creature_cards::UNITS_PER_FOOT;
use crate::frontend::gui::image_store;
use eframe::egui;

/// Anchor names offered up front; any other name can be added freely.
const SUGGESTED_ANCHORS: &[&str] = &["feet", "head", "mouth", "back", "saddle"];

/// Body types from the scale spec, each with the landmark the red target
/// line marks ("height" means different things per silhouette).
const BODY_TYPES: &[(&str, &str)] = &[
    ("Biped", "crown of head, standing erect"),
    ("Quadruped", "shoulder (withers) — head/neck may rise above"),
    ("Hybrid", "crown of the humanoid head/torso"),
    ("Plantlife", "topmost foliage/limb"),
    ("Elemental", "top of visual mass (not wisps/particles)"),
    ("Insect", "top of body, wings folded"),
    ("Arachnid", "top of cephalothorax in stance"),
    ("Ophidian", "body diameter at the thickest coil"),
    ("Worm", "body diameter (a reared pose may rise ~3×)"),
    ("Avian", "crown, standing, wings folded"),
    ("Globoid", "sphere diameter"),
    ("Crustacean", "top of carapace"),
];

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
    /// A `{token}_<suffix>` layer file (wound overlays, pose art other
    /// than `_prone`) — hidden from the picker unless "all layers" is on,
    /// so the list reads as one entry per creature, not one per layer.
    layer: bool,
    /// Sidecar present at scan time — the "already calibrated" marker.
    calibrated: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum CalMode {
    /// Drag/size the sprite against the ruler.
    Fit,
    /// Sprite locked; clicks place anchors.
    Anchors,
}

pub(crate) struct CreatureCalibrationState {
    choices: Vec<CreatureChoice>,
    /// Area groups over `choices` (bestiary `areas`; multi-area creatures
    /// appear under each; unknowns under "Other"), sorted by area name.
    groups: Vec<(String, Vec<usize>)>,
    /// Picker search box; non-empty flattens the groups to matches.
    search: String,
    selected: Option<usize>,
    texture: Option<egui::TextureHandle>,
    /// Alpha-content bbox of the selected image, as image fractions
    /// [x0, y0, x1, y1] — the fit sizes the CONTENT, matching the field
    /// renderer's authored-size semantics.
    bbox: [f32; 4],
    mode: CalMode,
    /// Fitted content height in feet — the number the whole stage turns
    /// into sidecar `size` on save.
    height_ft: f32,
    /// Content-center horizontal offset from stage center, in feet.
    pos_x_ft: f32,
    /// Content bottom edge above the ground line, in feet (negative =
    /// sunk below it).
    bottom_ft: f32,
    /// Stage zoom in px per foot; 0 = auto-fit next frame.
    zoom: f32,
    /// Index into [`BODY_TYPES`], driving the target-line note.
    body_type: usize,
    target_on: bool,
    /// Red dotted line height in feet (bestiary height when known).
    target_ft: f32,
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
    /// Write the fitted size to the sidecar on save. Off for images where
    /// only anchors/overlay-scale are being edited (shared overlay art).
    write_size: bool,
    lift_on: bool,
    lift: f32,
    /// Overlay art (creatures/wounds, creatures/status): drawn width as a
    /// fraction of the wearing creature's drawn width.
    overlay_scale_on: bool,
    overlay_scale: f32,
    /// Template-canvas calibration (scale-spec): px/ft authored into the
    /// canvas; world size derives from the art itself.
    template_on: bool,
    px_per_foot: f32,
    /// Show `{token}_<suffix>` layer files (wound overlays, pose art) in
    /// the picker instead of just base + `_prone` images.
    show_layers: bool,
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
            .map(|image| {
                let layer = is_layer_file(&image.abs_path);
                CreatureChoice {
                    label: image.display_label(),
                    pool_path: image.pool_path.clone(),
                    abs_path: image.abs_path.clone(),
                    layer,
                    calibrated: image.has_sidecar,
                }
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
        // Area grouping from the bestiary; a creature in several areas
        // lists under each, unknown names land in "Other".
        let db = crate::core::bestiary::format::shared();
        let mut by_area: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (index, choice) in choices.iter().enumerate() {
            let stem = choice.label.rsplit('/').next().unwrap_or(&choice.label).trim();
            let name = stem
                .strip_suffix("_prone")
                .unwrap_or(stem)
                .replace('_', " ");
            let areas = db.areas_for_name(&name);
            if areas.is_empty() {
                by_area.entry("Other".to_owned()).or_default().push(index);
            } else {
                for area in areas {
                    by_area.entry(area).or_default().push(index);
                }
            }
        }
        let groups: Vec<(String, Vec<usize>)> = by_area.into_iter().collect();

        let selected = (choices.len() == 1).then_some(0);
        let mut state = CreatureCalibrationState {
            choices,
            groups,
            search: String::new(),
            selected,
            texture: None,
            bbox: [0.0, 0.0, 1.0, 1.0],
            mode: CalMode::Fit,
            height_ft: 6.0,
            pos_x_ft: 0.0,
            bottom_ft: 0.0,
            zoom: 0.0,
            body_type: 0,
            target_on: false,
            target_ft: 6.0,
            anchors: HashMap::new(),
            selected_anchor: "feet".to_owned(),
            new_anchor_name: String::new(),
            footprint_on: false,
            rx: 0.35,
            ry_auto: true,
            ry: 0.35 * 0.24,
            footprint_center: None,
            write_size: true,
            lift_on: false,
            lift: 0.1,
            overlay_scale_on: false,
            overlay_scale: 1.0,
            template_on: false,
            px_per_foot: 32.0,
            show_layers: false,
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
            .default_width(720.0)
            .default_height(620.0)
            .resizable(true)
            .show(ctx, |ui| {
                let selected_label = state
                    .selected
                    .map(|index| state.choices[index].label.clone())
                    .unwrap_or_else(|| "Pick a creature below".to_owned());
                ui.horizontal(|ui| {
                    ui.strong(selected_label);
                    ui.separator();
                    ui.selectable_value(&mut state.mode, CalMode::Fit, "Fit")
                        .on_hover_text(
                            "Drag the creature and set its height until the landmark \
                             sits on the red line",
                        );
                    ui.selectable_value(&mut state.mode, CalMode::Anchors, "Anchors")
                        .on_hover_text(
                            "Sprite locked in place; click to place the selected anchor",
                        );
                });
                // Picker: search + area groups (bestiary `areas`), with a
                // • marker on images that already carry a calibration.
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.search)
                            .hint_text("search creatures")
                            .desired_width(180.0),
                    );
                    ui.checkbox(&mut state.show_layers, "all layers")
                        .on_hover_text(
                            "List every image file, including per-wound overlays and pose \
                             layers ({token}_chest1, …). Off = base + _prone images only.",
                        );
                });
                let search = state.search.trim().to_ascii_lowercase();
                let visible = |choice: &CreatureChoice, index: usize| {
                    (!choice.layer || state.show_layers || state.selected == Some(index))
                        && (search.is_empty()
                            || choice.label.to_ascii_lowercase().contains(&search))
                };
                egui::ScrollArea::vertical()
                    .id_salt("creature_cal_picker")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        let mut row = |ui: &mut egui::Ui, index: usize| {
                            let choice = &state.choices[index];
                            let label = if choice.calibrated {
                                format!("{} \u{2022}", choice.label)
                            } else {
                                choice.label.clone()
                            };
                            if ui
                                .selectable_label(state.selected == Some(index), label)
                                .on_hover_text(if choice.calibrated {
                                    "Calibrated (sidecar present)"
                                } else {
                                    "Not yet calibrated"
                                })
                                .clicked()
                            {
                                load_request = Some(index);
                            }
                        };
                        if search.is_empty() {
                            for (area, members) in &state.groups {
                                let shown: Vec<usize> = members
                                    .iter()
                                    .copied()
                                    .filter(|&i| visible(&state.choices[i], i))
                                    .collect();
                                if shown.is_empty() {
                                    continue;
                                }
                                egui::CollapsingHeader::new(format!(
                                    "{area} ({})",
                                    shown.len()
                                ))
                                .default_open(state.groups.len() == 1)
                                .show(ui, |ui| {
                                    for index in shown {
                                        row(ui, index);
                                    }
                                });
                            }
                        } else {
                            // Search flattens the groups to matches.
                            for index in 0..state.choices.len() {
                                if visible(&state.choices[index], index) {
                                    row(ui, index);
                                }
                            }
                        }
                    });

                let (Some(texture), Some(_)) = (&state.texture, state.selected) else {
                    return;
                };
                let tex_size = texture.size_vec2();
                let tex_id = texture.id();

                match state.mode {
                    CalMode::Fit => {
                        ui.horizontal(|ui| {
                            let type_label = BODY_TYPES[state.body_type].0;
                            egui::ComboBox::from_label("Body type")
                                .selected_text(type_label)
                                .show_ui(ui, |ui| {
                                    for (index, (name, _)) in BODY_TYPES.iter().enumerate() {
                                        ui.selectable_value(&mut state.body_type, index, *name);
                                    }
                                });
                            ui.checkbox(&mut state.target_on, "Target line");
                            if state.target_on {
                                ui.add(
                                    egui::DragValue::new(&mut state.target_ft)
                                        .range(0.1..=50.0)
                                        .speed(0.1)
                                        .suffix(" ft"),
                                )
                                .on_hover_text("Bestiary height when known; editable");
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Height");
                            ui.add(
                                egui::DragValue::new(&mut state.height_ft)
                                    .range(0.1..=60.0)
                                    .speed(0.05)
                                    .suffix(" ft"),
                            )
                            .on_hover_text(
                                "Drawn height of the art's content — scroll on the stage \
                                 also adjusts it",
                            );
                            if ui
                                .button("Ground")
                                .on_hover_text("Drop the content onto the ground line")
                                .clicked()
                            {
                                state.bottom_ft = 0.0;
                            }
                            if ui
                                .button("Match target")
                                .on_hover_text("Set height to the target line")
                                .clicked()
                            {
                                state.height_ft = state.target_ft;
                            }
                            ui.separator();
                            ui.label("Zoom");
                            let mut auto = state.zoom <= 0.0;
                            if ui
                                .checkbox(&mut auto, "auto")
                                .on_hover_text("Fit the stage to the creature + target")
                                .changed()
                            {
                                state.zoom = if auto { 0.0 } else { 16.0 };
                            }
                            if state.zoom > 0.0 {
                                ui.add(
                                    egui::Slider::new(&mut state.zoom, 2.0..=64.0)
                                        .suffix(" px/ft")
                                        .logarithmic(true),
                                );
                            }
                        });
                    }
                    CalMode::Anchors => {
                        ui.label(
                            "Click the sprite to place the selected anchor. feet grounds the \
                             sprite on the field; named anchors position status/wound overlay \
                             layers.",
                        );
                    }
                }
                ui.separator();

                ui.horizontal_top(|ui| {
                    // Anchor list: suggested + present + add-your-own
                    // (anchor mode only — fit mode gives the stage the room).
                    if state.mode == CalMode::Anchors {
                        ui.vertical(|ui| {
                            ui.set_width(160.0);
                            let mut names: Vec<String> =
                                SUGGESTED_ANCHORS.iter().map(|s| s.to_string()).collect();
                            for key in state.anchors.keys() {
                                if !names.iter().any(|n| n.eq_ignore_ascii_case(key))
                                    && !wound_anchor_names()
                                        .any(|w| w.eq_ignore_ascii_case(key))
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
                            // Wound parts: the injury overlays look these up
                            // by doll-part name, collapsed so the common
                            // anchors stay one glance.
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
                                let field =
                                    egui::TextEdit::singleline(&mut state.new_anchor_name)
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
                    }

                    // The ruler stage.
                    const CONTROLS_HEIGHT: f32 = 130.0;
                    let avail = ui.available_size();
                    let canvas = egui::Vec2::new(
                        avail.x.max(200.0),
                        (avail.y - CONTROLS_HEIGHT).max(240.0),
                    );
                    let sense = match state.mode {
                        CalMode::Fit => egui::Sense::click_and_drag(),
                        CalMode::Anchors => egui::Sense::click(),
                    };
                    let (rect, response) = ui.allocate_exact_size(canvas, sense);
                    let painter = ui.painter().with_clip_rect(rect);
                    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

                    // Auto zoom: creature + target inside ~80% of the stage.
                    let ground_y = rect.max.y - 20.0;
                    let span_ft = state
                        .height_ft
                        .max(state.target_on.then_some(state.target_ft).unwrap_or(0.0))
                        .max(1.0);
                    let zoom = if state.zoom > 0.0 {
                        state.zoom
                    } else {
                        ((ground_y - rect.min.y) * 0.8 / span_ft).clamp(1.0, 64.0)
                    };

                    // Ruler: a line per foot when legible, else per 5 ft;
                    // labels on the left every 5 ft (every foot when roomy).
                    let weak = ui.visuals().weak_text_color();
                    let grid = egui::Color32::from_rgba_unmultiplied(
                        weak.r(),
                        weak.g(),
                        weak.b(),
                        40,
                    );
                    let step = if zoom >= 5.0 { 1 } else { 5 };
                    let label_step = if zoom >= 24.0 {
                        1
                    } else if zoom >= 5.0 {
                        5
                    } else {
                        10
                    };
                    let mut ft = 0i32;
                    loop {
                        let y = ground_y - ft as f32 * zoom;
                        if y < rect.min.y {
                            break;
                        }
                        let major = ft % label_step == 0;
                        painter.line_segment(
                            [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                            egui::Stroke::new(
                                if ft == 0 { 1.5 } else { 1.0 },
                                if ft == 0 {
                                    egui::Color32::from_rgba_unmultiplied(120, 200, 120, 160)
                                } else if major {
                                    egui::Color32::from_rgba_unmultiplied(
                                        weak.r(),
                                        weak.g(),
                                        weak.b(),
                                        90,
                                    )
                                } else {
                                    grid
                                },
                            ),
                        );
                        if major {
                            painter.text(
                                egui::pos2(rect.min.x + 4.0, y - 2.0),
                                egui::Align2::LEFT_BOTTOM,
                                format!("{ft} ft"),
                                egui::FontId::proportional(10.0),
                                weak,
                            );
                        }
                        ft += step;
                    }

                    // The sprite, at the fitted transform. Content height =
                    // height_ft; content bottom sits bottom_ft above ground.
                    let content_h = ((state.bbox[3] - state.bbox[1]) * tex_size.y).max(1.0);
                    let scale = state.height_ft * zoom / content_h;
                    let drawn = tex_size * scale;
                    let content_bottom_y = ground_y - state.bottom_ft * zoom;
                    let image_top_y = content_bottom_y - state.bbox[3] * tex_size.y * scale;
                    let content_cx =
                        (state.bbox[0] + state.bbox[2]) / 2.0 * tex_size.x * scale;
                    let image_left =
                        rect.center().x + state.pos_x_ft * zoom - content_cx;
                    let dest = egui::Rect::from_min_size(
                        egui::pos2(image_left, image_top_y),
                        drawn,
                    );
                    painter.image(
                        tex_id,
                        dest,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );

                    // Red dotted target line with the landmark note.
                    if state.target_on {
                        let y = ground_y - state.target_ft * zoom;
                        let red = egui::Color32::from_rgb(220, 70, 70);
                        let mut x = rect.min.x;
                        while x < rect.max.x {
                            painter.line_segment(
                                [egui::pos2(x, y), egui::pos2((x + 6.0).min(rect.max.x), y)],
                                egui::Stroke::new(1.5, red),
                            );
                            x += 11.0;
                        }
                        painter.text(
                            egui::pos2(rect.max.x - 6.0, y - 3.0),
                            egui::Align2::RIGHT_BOTTOM,
                            format!(
                                "{:.1} ft — {}",
                                state.target_ft, BODY_TYPES[state.body_type].1
                            ),
                            egui::FontId::proportional(11.0),
                            red,
                        );
                    }

                    match state.mode {
                        CalMode::Fit => {
                            // Drag moves; scroll over the stage resizes.
                            if response.dragged() {
                                let delta = response.drag_delta();
                                state.pos_x_ft += delta.x / zoom;
                                state.bottom_ft -= delta.y / zoom;
                            }
                            if response.hovered() {
                                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                                if scroll.abs() > 0.0 {
                                    state.height_ft = (state.height_ft
                                        * (1.0 + scroll * 0.002))
                                        .clamp(0.1, 60.0);
                                }
                            }
                        }
                        CalMode::Anchors => {
                            let at = |anchor: [f32; 2]| {
                                dest.min
                                    + egui::Vec2::new(
                                        anchor[0] * dest.width(),
                                        anchor[1] * dest.height(),
                                    )
                            };
                            let feet = state
                                .anchors
                                .get("feet")
                                .copied()
                                .unwrap_or([0.5, state.bbox[3]]);
                            // Footprint ellipse under the feet anchor.
                            if state.footprint_on {
                                let rx = state.rx * dest.width();
                                let ry = if state.ry_auto {
                                    state.rx * 0.24
                                } else {
                                    state.ry
                                } * dest.width();
                                // Shadow centre X: authored centre wins, else feet.
                                let shadow_cx = state
                                    .footprint_center
                                    .map(|[x, _]| at([x, 0.0]).x)
                                    .unwrap_or_else(|| at(feet).x);
                                let center = egui::pos2(shadow_cx, at(feet).y);
                                paint_ellipse(
                                    &painter,
                                    center,
                                    rx,
                                    ry,
                                    egui::Stroke::new(
                                        1.5,
                                        egui::Color32::from_rgba_unmultiplied(
                                            120, 200, 120, 200,
                                        ),
                                    ),
                                );
                            }
                            // All anchors; the selected one cross-haired.
                            let highlight = ui.visuals().hyperlink_color;
                            for (name, anchor) in &state.anchors {
                                let pos = at(*anchor);
                                let selected =
                                    name.eq_ignore_ascii_case(&state.selected_anchor);
                                let color = if selected {
                                    highlight
                                } else {
                                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 200)
                                };
                                painter.circle_stroke(
                                    pos,
                                    4.0,
                                    egui::Stroke::new(1.5, color),
                                );
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
                                        egui::pos2(rect.min.x, center.y),
                                        egui::pos2(rect.max.x, center.y),
                                    ],
                                    stroke,
                                );
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x, rect.min.y),
                                        egui::pos2(center.x, rect.max.y),
                                    ],
                                    stroke,
                                );
                            }
                            if response.clicked() {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    if dest.contains(pos)
                                        && dest.width() > 0.0
                                        && dest.height() > 0.0
                                    {
                                        let normalized = [
                                            ((pos.x - dest.min.x) / dest.width())
                                                .clamp(0.0, 1.0),
                                            ((pos.y - dest.min.y) / dest.height())
                                                .clamp(0.0, 1.0),
                                        ];
                                        let key =
                                            state.selected_anchor.to_ascii_lowercase();
                                        // A hand-placed feet anchor IS the
                                        // grounding: sync the fit so the
                                        // save's fit-derived feet agrees
                                        // instead of clobbering it.
                                        if key == "feet" {
                                            state.bottom_ft = bottom_ft_for_feet(
                                                normalized[1],
                                                state.bbox,
                                                state.height_ft,
                                            );
                                        }
                                        state.anchors.insert(key, normalized);
                                    }
                                }
                            }
                        }
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.write_size, "Write size").on_hover_text(
                        "Save the fitted height as this image's world size. Uncheck for \
                         shared overlay art where only anchors/overlay scale matter.",
                    );
                    ui.weak(format!(
                        "{:.1} ft = {:.2} world units",
                        state.height_ft,
                        state.height_ft * UNITS_PER_FOOT
                    ));
                    ui.separator();
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
                    ui.checkbox(&mut state.template_on, "Template canvas")
                        .on_hover_text(
                            "Art authored on the scale-spec canvas (docs/art/scale-spec.md): \
                             pixels are feet at this ratio, baseline on the bottom edge. \
                             World size derives from the art itself; a written size still \
                             wins if both are set.",
                        );
                    if state.template_on {
                        ui.add(
                            egui::DragValue::new(&mut state.px_per_foot)
                                .range(1.0..=512.0)
                                .speed(1.0)
                                .suffix(" px/ft"),
                        );
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Save")
                        .on_hover_text(
                            "Writes the fit (size + grounding), anchors, footprint and lift \
                             to the image's sidecar (and embeds them in the PNG) — the \
                             calibration travels with the file",
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
                // The fitted grounding becomes the feet anchor: where the
                // ground line crosses the image at the current fit (x kept
                // from a placed anchor; content-center otherwise).
                let mut anchors = state.anchors.clone();
                if state.write_size {
                    let content_px = ((state.bbox[3] - state.bbox[1])
                        * state.texture.as_ref().map(|t| t.size_vec2().y).unwrap_or(1.0))
                    .max(1.0);
                    let tex_h = state
                        .texture
                        .as_ref()
                        .map(|t| t.size_vec2().y)
                        .unwrap_or(1.0)
                        .max(1.0);
                    let feet_y = (state.bbox[3]
                        + state.bottom_ft * content_px
                            / (tex_h * state.height_ft.max(0.01)))
                    .clamp(0.0, 1.0);
                    let feet_x = anchors
                        .get("feet")
                        .map(|a| a[0])
                        .unwrap_or((state.bbox[0] + state.bbox[2]) / 2.0);
                    anchors.insert("feet".to_owned(), [feet_x, feet_y]);
                }
                // Scenery calibration (exclusion edges + aspect) lives in
                // the same sidecar; carry it through untouched.
                let existing: CreatureSidecar =
                    pool::read_sidecar(&state.choices[index].abs_path).unwrap_or_default();
                let sidecar = state.to_sidecar(anchors, &existing);
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

/// Whether a pool file is a `{token}_<suffix>` layer beside its base
/// (wound overlays like `kobold_chest1`, pose layers like
/// `kobold_prone_chest1`) rather than something to calibrate directly.
/// The base's stem is its parent folder's name (the tier scheme); the
/// `_prone` pose itself stays visible — it carries its own calibration.
fn is_layer_file(path: &std::path::Path) -> bool {
    let (Some(stem), Some(parent)) = (
        path.file_stem().and_then(|s| s.to_str()),
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str()),
    ) else {
        return false;
    };
    let Some(suffix) = stem.strip_prefix(parent).and_then(|s| s.strip_prefix('_')) else {
        return false;
    };
    !suffix.eq_ignore_ascii_case("prone")
}

/// The stage grounding (content-bottom height above the ground line, in
/// feet) implied by a feet-anchor Y at the current fit. Exact inverse of
/// the save mapping in [`CreatureCalibrationState::ui`]: the renderer
/// puts the feet fraction ON the floor, so an anchor below the content
/// bottom floats the content and one above it sinks it.
fn bottom_ft_for_feet(feet_y: f32, bbox: [f32; 4], height_ft: f32) -> f32 {
    let content = (bbox[3] - bbox[1]).max(0.001);
    (feet_y - bbox[3]) / content * height_ft
}

/// Bestiary lookup for a pool image: folder/file stems are slugs of the
/// creature name (underscores for spaces), so de-slug and match by noun
/// with the exact-name-first discipline the field itself uses.
fn bestiary_for_label(label: &str) -> Option<(Option<f32>, Option<String>)> {
    // Pose/layer stems carry suffixes ("{token}_prone") — the creature
    // name is the folder segment when present, else the suffix-stripped
    // stem. Try the most specific candidate first.
    let stem = label.rsplit('/').next().unwrap_or(label).trim();
    let folder = label.split('/').next().filter(|_| label.contains('/'));
    let stripped = stem.strip_suffix("_prone").unwrap_or(stem);
    let db = crate::core::bestiary::format::shared();
    for candidate in [Some(stripped), folder.map(str::trim)].into_iter().flatten() {
        let name = candidate.replace('_', " ").to_ascii_lowercase();
        let Some(noun) = name.split_whitespace().last().map(str::to_string) else {
            continue;
        };
        let entries = db.by_noun(&noun);
        if let Some(entry) = entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&name))
            .or_else(|| (entries.len() == 1).then(|| &entries[0]))
        {
            return Some((
                entry.height.map(|h| h as f32),
                entry.creature_type.clone(),
            ));
        }
    }
    None
}

fn load_creature_choice(state: &mut CreatureCalibrationState, index: usize) {
    let choice = &state.choices[index];
    // The texture reloads lazily from render (`ensure_texture` needs ctx).
    state.texture = None;
    state.bbox = alpha_bbox(&choice.abs_path).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let label = choice.label.clone();
    let sidecar: CreatureSidecar = pool::read_sidecar(&choice.abs_path).unwrap_or_default();
    state.apply_sidecar(&sidecar);
    // Fit state: saved size, else the bestiary height, else the human 6 ft.
    // Prone art's content height is body THICKNESS, not standing height —
    // default to the prone-box ratio of the bestiary height so the first
    // fit starts in the right ballpark.
    let prone_pose = state.choices[index]
        .abs_path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.to_ascii_lowercase().ends_with("_prone"));
    let (bestiary_ft, bestiary_type) = bestiary_for_label(&label).unwrap_or((None, None));
    // Prone starting ratios from live calibration sessions: a curled biped
    // fits ~0.28 of standing (6 ft vampire → 1.7 ft); a felled quadruped
    // keeps most of its bulk (30 ft mastodon → 20 ft lying).
    let quad = bestiary_type
        .as_deref()
        .is_some_and(|t| t.trim().eq_ignore_ascii_case("quadruped"));
    let prone_ratio = if quad { 0.65 } else { 0.28 };
    let default_ft = match (bestiary_ft, prone_pose) {
        (Some(ft), true) => ft * prone_ratio,
        (Some(ft), false) => ft,
        (None, true) => 1.7,
        (None, false) => 6.0,
    };
    state.height_ft = sidecar
        .size
        .map(|s| s / UNITS_PER_FOOT)
        .unwrap_or(default_ft);
    state.target_ft = bestiary_ft.unwrap_or(state.height_ft);
    state.target_on = bestiary_ft.is_some();
    state.body_type = bestiary_type
        .and_then(|t| {
            BODY_TYPES
                .iter()
                .position(|(name, _)| name.eq_ignore_ascii_case(t.trim()))
        })
        .unwrap_or(0);
    state.pos_x_ft = 0.0;
    // Grounding from the saved feet anchor (inverse of the save mapping):
    // a feet anchor BELOW the content bottom (in the padding) means the
    // content floats above the ground by that much; above it, it sinks.
    // No anchor = content bottom on the ground.
    state.bottom_ft = 0.0;
    if let Some(feet) = state.anchors.get("feet") {
        state.bottom_ft = bottom_ft_for_feet(feet[1], state.bbox, state.height_ft);
    }
    state.zoom = 0.0; // auto-fit
    state.write_size = true;
}

impl CreatureCalibrationState {
    /// Adopt a loaded sidecar's authored fields into the editor state.
    /// Everything authored — including the footprint centre — is retained
    /// for round-tripping. Fit state (height/grounding) is derived
    /// separately in [`load_creature_choice`].
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
        self.lift_on = sidecar.lift.is_some();
        if let Some(lift) = sidecar.lift {
            self.lift = lift;
        }
        self.overlay_scale_on = sidecar.overlay_scale.is_some();
        if let Some(scale) = sidecar.overlay_scale {
            self.overlay_scale = scale;
        }
        self.template_on = sidecar.px_per_foot.is_some();
        if let Some(ppf) = sidecar.px_per_foot {
            self.px_per_foot = ppf;
        }
    }

    /// The sidecar this editor state saves. `anchors` is the working set
    /// (the save path folds the fitted grounding into the feet anchor
    /// first); `existing` carries through the scenery-calibration fields
    /// (exclude/aspect) that live in the same file. The footprint centre
    /// round-trips unless the user explicitly reset it to the feet anchor.
    fn to_sidecar(
        &self,
        anchors: HashMap<String, [f32; 2]>,
        existing: &CreatureSidecar,
    ) -> CreatureSidecar {
        CreatureSidecar {
            kind: None, // the writer stamps it
            anchors,
            footprint: self.footprint_on.then(|| CreatureFootprint {
                rx: self.rx,
                ry: (!self.ry_auto).then_some(self.ry),
                center: self.footprint_center,
            }),
            size: self.write_size.then_some(self.height_ft * UNITS_PER_FOOT),
            px_per_foot: self
                .template_on
                .then_some(self.px_per_foot)
                .filter(|ppf| *ppf > 0.0),
            lift: self.lift_on.then_some(self.lift),
            overlay_scale: self.overlay_scale_on.then_some(self.overlay_scale),
            exclude: existing.exclude,
            aspect: existing.aspect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_state() -> CreatureCalibrationState {
        CreatureCalibrationState {
            choices: Vec::new(),
            groups: Vec::new(),
            search: String::new(),
            selected: None,
            texture: None,
            bbox: [0.0, 0.0, 1.0, 1.0],
            mode: CalMode::Fit,
            height_ft: 6.0,
            pos_x_ft: 0.0,
            bottom_ft: 0.0,
            zoom: 0.0,
            body_type: 0,
            target_on: false,
            target_ft: 6.0,
            anchors: HashMap::new(),
            selected_anchor: "feet".to_owned(),
            new_anchor_name: String::new(),
            footprint_on: false,
            rx: 0.35,
            ry_auto: true,
            ry: 0.35 * 0.24,
            footprint_center: None,
            write_size: false,
            lift_on: false,
            lift: 0.1,
            overlay_scale_on: false,
            overlay_scale: 1.0,
            template_on: false,
            px_per_foot: 32.0,
            show_layers: false,
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
    fn feet_anchor_and_fit_grounding_are_exact_inverses() {
        // The save path derives feet_y from bottom_ft; the load path (and
        // a hand-placed feet click) derive bottom_ft from feet_y. A sign
        // slip here made every save invert the grounding (a placed contact
        // point became an equal-sized float). Pin the round trip.
        let bbox = [0.1_f32, 0.2, 0.9, 0.8];
        let (tex_h, height_ft) = (100.0_f32, 6.0_f32);
        let content_frac = bbox[3] - bbox[1];
        for feet_y in [0.5_f32, 0.8, 0.95] {
            let bottom_ft = bottom_ft_for_feet(feet_y, bbox, height_ft);
            // Mirror of the save formula in ui().
            let content_px = content_frac * tex_h;
            let saved_feet_y = bbox[3] + bottom_ft * content_px / (tex_h * height_ft);
            assert!(
                (saved_feet_y - feet_y).abs() < 1e-5,
                "feet {feet_y} -> bottom {bottom_ft} -> feet {saved_feet_y}"
            );
        }
        // Signs: anchor above the content bottom sinks; below it floats.
        assert!(bottom_ft_for_feet(0.5, bbox, height_ft) < 0.0);
        assert!(bottom_ft_for_feet(0.95, bbox, height_ft) > 0.0);
    }

    #[test]
    fn footprint_center_roundtrips_through_unrelated_edits() {
        let mut state = blank_state();
        state.apply_sidecar(&authored_sidecar());
        assert_eq!(state.footprint_center, Some([0.3, 0.6]));
        // Change only the head anchor — the authored centre (X and the
        // stored, unrendered Y) must survive the save untouched.
        state.anchors.insert("head".to_string(), [0.55, 0.05]);
        let saved = state.to_sidecar(state.anchors.clone(), &CreatureSidecar::default());
        let fp = saved.footprint.unwrap();
        assert_eq!(fp.center, Some([0.3, 0.6]));
        assert_eq!(fp.rx, 0.46);
        assert_eq!(fp.ry, Some(0.12));
        assert_eq!(saved.anchors["head"], [0.55, 0.05]);
        // Explicit reset to the feet anchor drops the authored centre.
        state.footprint_center = None;
        let reset = state.to_sidecar(state.anchors.clone(), &CreatureSidecar::default());
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
        pool::write_creature_sidecar(
            &image_path,
            &state.to_sidecar(state.anchors.clone(), &loaded),
        )
        .unwrap();

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

/// Alpha-content bbox of an image as fractions, same 32/255 threshold as
/// the field loader — the fit sizes content, not canvas.
fn alpha_bbox(path: &std::path::Path) -> Option<[f32; 4]> {
    let rgba = image::open(path).ok()?.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if w == 0 || h == 0 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, px) in rgba.enumerate_pixels() {
        if px.0[3] >= 32 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 <= x1 && y0 <= y1).then(|| {
        [
            x0 as f32 / w as f32,
            y0 as f32 / h as f32,
            (x1 + 1) as f32 / w as f32,
            (y1 + 1) as f32 / h as f32,
        ]
    })
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
