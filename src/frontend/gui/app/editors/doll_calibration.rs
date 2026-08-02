//! Injury doll calibrator: click the active skin's doll image to place
//! each body part's dot anchor, style the generated wound/scar dots, and
//! save the result into the skin's skin.toml (comments preserved). Saved
//! coordinates are fractions of the base image, so they hold at any
//! window size or zoom.

use std::collections::HashMap;

use super::super::VellumGuiApp;
use super::color_field;
use crate::config::skins::{self, DollDotSpec, DOLL_PARTS};
use crate::frontend::gui::skin::{self as gui_skin, ResolvedDotStyle};
use eframe::egui;

/// Where the calibration saves: a skin's skin.toml, or a pool doll's
/// sidecar toml (calibration travels with the artwork).
pub(in super::super) enum CalibrationTarget {
    Skin(String),
    Pool {
        image_abs: std::path::PathBuf,
        /// Display name (image stem) for the window title and messages.
        label: String,
    },
}

impl CalibrationTarget {
    fn label(&self) -> &str {
        match self {
            CalibrationTarget::Skin(name) => name,
            CalibrationTarget::Pool { label, .. } => label,
        }
    }
}

pub(in super::super) struct DollCalibrationState {
    /// Where Save writes.
    target: CalibrationTarget,
    /// Working anchors keyed by lowercase protocol part name. Only parts
    /// the skin (or this session) has placed appear here; the rest render
    /// at built-in defaults and stay implicit in the saved file.
    anchors: HashMap<String, [f32; 2]>,
    /// Index into DOLL_PARTS of the part the next click places.
    selected: usize,
    /// Jump to the next part after each click.
    auto_advance: bool,
    wound_color: String,
    scar_color: String,
    opacity: f32,
    diameter: f32,
    /// Preview dots as scars (rings) instead of wounds (solid).
    preview_scars: bool,
    /// Preview severity numeral, 1-3.
    preview_level: u8,
    error: Option<String>,
}

impl DollCalibrationState {
    fn dot_spec(&self) -> DollDotSpec {
        DollDotSpec {
            wound_color: self.wound_color.trim().to_string(),
            scar_color: self.scar_color.trim().to_string(),
            opacity: self.opacity,
            diameter: self.diameter,
        }
    }

    fn anchor_for(&self, part_key: &str) -> egui::Vec2 {
        let key = part_key.to_ascii_lowercase();
        self.anchors
            .get(&key)
            .copied()
            .or_else(|| skins::default_doll_anchor(&key))
            .map(|[x, y]| egui::vec2(x, y))
            .unwrap_or(egui::vec2(0.5, 0.5))
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_doll_calibration(&mut self) {
        // Already calibrating: raise rather than re-read the manifest, so
        // in-progress dot placement isn't reset.
        if self.doll_calibration.is_some() {
            self.raise_editor(egui::Id::new("gui_doll_calibration"));
            return;
        }
        // A doll override calibrates against its pool sidecar and needs no
        // skin at all; otherwise calibration targets the active skin.
        let (target, anchors, dots) = if let (Some(pool_path), Some(abs)) = (
            self.skin_state.doll_override().map(str::to_owned),
            self.skin_state.doll_override_abs_path(),
        ) {
            let sidecar: crate::config::pool::DollSidecar =
                crate::config::pool::read_sidecar(&abs).unwrap_or_default();
            let label = pool_path
                .rsplit_once('/')
                .map(|(_, file)| file)
                .unwrap_or(&pool_path)
                .rsplit_once('.')
                .map(|(stem, _)| stem.to_owned())
                .unwrap_or(pool_path.clone());
            (
                CalibrationTarget::Pool { image_abs: abs, label },
                sidecar.anchors,
                sidecar.dots,
            )
        } else {
            let Some(skin_name) = self.skin_state.loaded_skin().map(str::to_owned) else {
                self.app_core.add_system_message(
                    "No doll image selected. Pick one in the injuries window's Appearance menu \
                     (install some with .jinx), or activate a skin with a doll.",
                );
                return;
            };
            let has_base = self
                .skin_state
                .widget_art()
                .is_some_and(|art| art.doll_base.is_some());
            if !has_base {
                self.app_core.add_system_message(&format!(
                    "Skin '{}' has no injury doll base image; pick a doll in the injuries \
                     window's Appearance menu or set base under [injury_doll] in its skin.toml.",
                    skin_name
                ));
                return;
            }
            let doll = self.skin_state.doll_manifest();
            (
                CalibrationTarget::Skin(skin_name),
                doll.anchors.clone(),
                doll.dots.clone(),
            )
        };
        let anchors = anchors
            .iter()
            .map(|(part, anchor)| (part.to_ascii_lowercase(), *anchor))
            .collect();
        self.doll_calibration = Some(DollCalibrationState {
            target,
            anchors,
            selected: 0,
            auto_advance: true,
            wound_color: dots.wound_color,
            scar_color: dots.scar_color,
            opacity: dots.opacity,
            diameter: dots.diameter,
            preview_scars: false,
            preview_level: 2,
            error: None,
        });
    }

    pub(in super::super) fn render_doll_calibration(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.doll_calibration.take() else {
            return;
        };
        // The base can vanish mid-session (skin.toml edited on disk, doll
        // override cleared); close rather than calibrating against nothing.
        let Some(base) = self.skin_state.widget_art().and_then(|art| art.doll_base) else {
            self.app_core.add_system_message(
                "Injury doll calibration closed: no doll base image is loaded any more.",
            );
            return;
        };

        let mut open = true;
        let mut save_request = false;

        egui::Window::new(format!("Injury Doll Calibration - {}", state.target.label()))
            .id(egui::Id::new("gui_doll_calibration"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(560.0)
            .default_height(520.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Click the doll to place the highlighted part's dot. Coordinates are stored as fractions of the image, so they hold at any size.");
                ui.separator();
                ui.horizontal_top(|ui| {
                    // Part list: click-through order, dot marker for parts
                    // with a calibrated (non-default) anchor.
                    ui.vertical(|ui| {
                        ui.set_width(150.0);
                        for (index, (key, display, _)) in DOLL_PARTS.iter().enumerate() {
                            let calibrated =
                                state.anchors.contains_key(&key.to_ascii_lowercase());
                            let label = if calibrated {
                                format!("{display} \u{2022}")
                            } else {
                                display.to_string()
                            };
                            if ui
                                .selectable_label(state.selected == index, label)
                                .clicked()
                            {
                                state.selected = index;
                            }
                        }
                        ui.add_space(6.0);
                        ui.checkbox(&mut state.auto_advance, "Auto-advance")
                            .on_hover_text("Jump to the next part after each click");
                        if ui
                            .button("Use default")
                            .on_hover_text(
                                "Drop the selected part's calibration and fall back to the built-in position",
                            )
                            .clicked()
                        {
                            let key = DOLL_PARTS[state.selected].0.to_ascii_lowercase();
                            state.anchors.remove(&key);
                        }
                    });

                    // Doll canvas: base image aspect-fit into whatever space
                    // is left, every part's dot drawn live at preview
                    // severity, the selected part cross-haired. Reserve room
                    // below for the style rows and the Save button so the
                    // canvas can't push them past the window's bottom edge.
                    const CONTROLS_HEIGHT: f32 = 130.0;
                    let avail = ui.available_size();
                    let canvas = egui::Vec2::new(
                        avail.x.max(160.0),
                        (avail.y - CONTROLS_HEIGHT).max(200.0),
                    );
                    let (rect, response) =
                        ui.allocate_exact_size(canvas, egui::Sense::click());
                    let painter = ui.painter().with_clip_rect(rect);
                    painter.rect_filled(
                        rect,
                        4.0,
                        ui.visuals().extreme_bg_color,
                    );
                    let dest = gui_skin::sprite_dest(&base, rect);
                    gui_skin::paint_sprite(&painter, dest, &base, egui::Color32::WHITE);

                    let style = ResolvedDotStyle::from_spec(&state.dot_spec());
                    let level = state.preview_level.clamp(1, 3)
                        + if state.preview_scars { 3 } else { 0 };
                    let radius = (style.diameter * dest.height() / 2.0).max(4.0);
                    let at = |anchor: egui::Vec2| {
                        dest.min
                            + egui::Vec2::new(anchor.x * dest.width(), anchor.y * dest.height())
                    };
                    for (key, _, _) in DOLL_PARTS {
                        gui_skin::paint_severity_dot(
                            &painter,
                            at(state.anchor_for(key)),
                            radius,
                            level,
                            &style,
                        );
                    }

                    // Selected-part crosshair on top of its dot.
                    let highlight = ui.visuals().hyperlink_color;
                    let center = at(state.anchor_for(DOLL_PARTS[state.selected].0));
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
                    painter.circle_stroke(
                        center,
                        radius + 3.0,
                        egui::Stroke::new(2.0, highlight),
                    );

                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            if dest.contains(pos) && dest.width() > 0.0 && dest.height() > 0.0 {
                                let normalized = [
                                    ((pos.x - dest.min.x) / dest.width()).clamp(0.0, 1.0),
                                    ((pos.y - dest.min.y) / dest.height()).clamp(0.0, 1.0),
                                ];
                                let key = DOLL_PARTS[state.selected].0.to_ascii_lowercase();
                                state.anchors.insert(key, normalized);
                                if state.auto_advance {
                                    state.selected = (state.selected + 1) % DOLL_PARTS.len();
                                }
                            }
                        }
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Wound");
                    color_field(ui, &mut state.wound_color);
                    ui.label("Scar");
                    color_field(ui, &mut state.scar_color);
                    ui.label("Preview:");
                    ui.selectable_value(&mut state.preview_scars, false, "wounds");
                    ui.selectable_value(&mut state.preview_scars, true, "scars");
                    ui.add(
                        egui::Slider::new(&mut state.preview_level, 1..=3).text("rank"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.diameter, 0.02..=0.20)
                            .text("dot size")
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.opacity, 0.2..=1.0)
                            .text("opacity")
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                    );
                });

                ui.separator();
                ui.horizontal(|ui| {
                    let (save_label, save_hover) = match &state.target {
                        CalibrationTarget::Skin(_) => (
                            "Save to skin",
                            "Writes [injury_doll.anchors] and [injury_doll.dots] into the skin's skin.toml",
                        ),
                        CalibrationTarget::Pool { .. } => (
                            "Save",
                            "Writes anchors and dot styling into the doll's sidecar toml, next to the image — the calibration travels with the artwork",
                        ),
                    };
                    if ui.button(save_label).on_hover_text(save_hover).clicked() {
                        save_request = true;
                    }
                    if ui
                        .button("Reset all to defaults")
                        .clicked()
                    {
                        state.anchors.clear();
                        let defaults = DollDotSpec::default();
                        state.wound_color = defaults.wound_color;
                        state.scar_color = defaults.scar_color;
                        state.opacity = defaults.opacity;
                        state.diameter = defaults.diameter;
                    }
                });
                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });

        if save_request {
            let result = match &state.target {
                CalibrationTarget::Skin(skin) => {
                    gui_skin::save_calibration(skin, &state.anchors, &state.dot_spec())
                }
                CalibrationTarget::Pool { image_abs, .. } => {
                    crate::config::pool::write_doll_sidecar(
                        image_abs,
                        &state.anchors,
                        &state.dot_spec(),
                    )
                }
            };
            match result {
                Ok(()) => {
                    state.error = None;
                    // Sidecars aren't mtime-polled (and the skin poll takes a
                    // second); force a reload so the live doll updates on the
                    // very next frame.
                    self.skin_state.force_reload();
                    self.app_core.add_system_message(&format!(
                        "Injury doll calibration saved to '{}'.",
                        state.target.label()
                    ));
                }
                Err(err) => {
                    state.error = Some(format!("Failed to save: {}", err));
                }
            }
        }

        if open {
            self.doll_calibration = Some(state);
        }
    }
}
