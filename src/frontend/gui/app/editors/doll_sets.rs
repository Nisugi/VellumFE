//! Injury doll workbench: one window that configures a whole doll.
//!
//! Pick a base doll and everything about it is here — its wound/scar
//! overlay layers (discovered beside it by filename, toggled on and off to
//! see what each looks like), the dot anchors for parts that have no
//! layer art, the dot styling, and the variants that swap the whole doll
//! by game state.
//!
//! The organizing idea is that a VARIANT IS A PEER OF THE BASE. A prone
//! doll is not a patch on the standing one — it is its own art with its
//! own anchors and its own layers — so the editor switches between them
//! and offers the identical tooling on each, rather than treating variants
//! as a separate kind of thing configured somewhere else.
//!
//! Storage is each doll's own sidecar (`pool::DollSidecar`): anchors,
//! dots, and the `when` rule that selects it. Layers need no storage at
//! all — the filename IS the assignment (`<base>_<part>_<severity>.png`),
//! so art dropped in the pool works with no wiring.

use std::collections::{HashMap, HashSet};

use super::super::VellumGuiApp;
use super::CalibrationOutcome;
use crate::config::pool::{self, DollSidecar};
use crate::config::skins::{self, DollDotSpec, DOLL_PARTS};
use crate::config::Condition;
use crate::frontend::gui::skin::{self as gui_skin, ResolvedDotStyle, SkinTexture};
use crate::frontend::gui::image_store;
use eframe::egui;

/// Fixed width of the right-hand controls column.
const SIDE_PANEL_WIDTH: f32 = 290.0;
/// The canvas never shrinks below this, even in a very narrow window; the
/// side panel gives up space first.
const MIN_CANVAS_WIDTH: f32 = 220.0;
const MIN_CANVAS_HEIGHT: f32 = 200.0;
/// Vertical room reserved under the split for a variant's rule editor.
const RULE_STRIP_HEIGHT: f32 = 150.0;

/// One overlay layer with its loaded texture and preview toggle.
struct Layer {
    part: String,
    level: u8,
    pool_path: String,
    abs_path: std::path::PathBuf,
    texture: Option<egui::TextureHandle>,
    /// Drawn in the preview right now.
    shown: bool,
}

impl Layer {
    /// "chest / wound 2", the label the toggle list shows.
    fn label(&self) -> String {
        let part_label = DOLL_PARTS
            .iter()
            .find(|(key, _, _)| key.eq_ignore_ascii_case(&self.part))
            .map(|(_, label, _)| *label)
            .unwrap_or(self.part.as_str());
        let severity = match self.level {
            0 => "healthy".to_owned(),
            1..=3 => format!("wound {}", self.level),
            _ => format!("scar {}", self.level - 3),
        };
        format!("{part_label} / {severity}")
    }
}

/// One doll being edited — the base or any variant. Same shape for both,
/// because they are peers: each owns its art, anchors, dots and rule.
struct DollEntry {
    /// Display name (the image stem).
    label: String,
    pool_path: String,
    abs_path: std::path::PathBuf,
    base: Option<egui::TextureHandle>,
    layers: Vec<Layer>,
    /// Lowercase part -> anchor in image fractions.
    anchors: HashMap<String, [f32; 2]>,
    wound_color: String,
    scar_color: String,
    opacity: f32,
    diameter: f32,
    /// Selection rule; `None` on the base (it is the fallback), `Some` on
    /// a variant.
    when: Option<Condition>,
    priority: i32,
    /// Doll name whose layer set this one renders — itself when it draws
    /// its own art, else its shared group.
    layer_owner: Option<String>,
    dirty: bool,
}

impl DollEntry {
    /// Load one pool doll and everything that belongs to it.
    fn load(image: &pool::PoolImage) -> Self {
        let sidecar: DollSidecar = pool::read_sidecar(&image.abs_path).unwrap_or_default();
        let anchors = sidecar
            .anchors
            .iter()
            .map(|(part, anchor)| (part.to_ascii_lowercase(), *anchor))
            .collect();
        let layers = pool::doll_layers(&image.pool_path)
            .into_iter()
            .map(|layer| Layer {
                part: layer.part,
                level: layer.level,
                pool_path: layer.pool_path,
                abs_path: layer.abs_path,
                texture: None,
                shown: false,
            })
            .collect();
        DollEntry {
            label: image.stem().to_owned(),
            pool_path: image.pool_path.clone(),
            abs_path: image.abs_path.clone(),
            base: None,
            layers,
            anchors,
            wound_color: sidecar.dots.wound_color,
            scar_color: sidecar.dots.scar_color,
            opacity: sidecar.dots.opacity,
            diameter: sidecar.dots.diameter,
            when: sidecar.when,
            priority: sidecar.priority,
            layer_owner: pool::doll_layer_owner(&image.pool_path),
            dirty: false,
        }
    }

    fn dot_spec(&self) -> DollDotSpec {
        DollDotSpec {
            wound_color: self.wound_color.trim().to_owned(),
            scar_color: self.scar_color.trim().to_owned(),
            opacity: self.opacity,
            diameter: self.diameter,
        }
    }

    /// Calibrated anchor, else the built-in default, else dead center.
    fn anchor_for(&self, part: &str) -> egui::Vec2 {
        let key = part.to_ascii_lowercase();
        self.anchors
            .get(&key)
            .copied()
            .or_else(|| skins::default_doll_anchor(&key))
            .map(|[x, y]| egui::vec2(x, y))
            .unwrap_or(egui::vec2(0.5, 0.5))
    }

    /// Parts that have layer art at any severity. These render hand-drawn,
    /// so the editor must NOT offer a dot anchor for them — the renderer
    /// would ignore it.
    fn parts_with_art(&self) -> HashSet<String> {
        self.layers.iter().map(|l| l.part.clone()).collect()
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        if self.base.is_none() {
            self.base = image_store::load_texture_file(
                ctx,
                &self.abs_path,
                &format!("doll-wb:{}", self.pool_path),
                "doll workbench",
            );
        }
        for layer in &mut self.layers {
            if layer.texture.is_none() {
                // The discovered path, not one rebuilt from the doll's:
                // a shared group's layers sit beside the GROUP, which need
                // not be the same folder as this doll.
                layer.texture = image_store::load_texture_file(
                    ctx,
                    &layer.abs_path,
                    &format!("doll-wb:{}", layer.pool_path),
                    "doll workbench layer",
                );
            }
        }
    }

    fn save(&mut self) -> anyhow::Result<()> {
        pool::write_doll_sidecar(&self.abs_path, &self.anchors, &self.dot_spec())?;
        pool::write_doll_variant(&self.abs_path, self.when.as_ref(), self.priority)?;
        self.dirty = false;
        Ok(())
    }
}

pub(crate) struct DollSetsEditorState {
    /// Every pool doll, any of which can be the base or a variant.
    entries: Vec<DollEntry>,
    /// Index of the doll whose art is the BASE of the set being edited.
    base: Option<usize>,
    /// Indices of the dolls acting as variants of that base, in evaluation
    /// order.
    variants: Vec<usize>,
    /// Which entry the editing surface is showing: `None` = the base,
    /// `Some(i)` = `variants[i]`.
    editing: Option<usize>,
    /// Part whose anchor the next canvas click places.
    selected_part: usize,
    auto_advance: bool,
    /// Severity the preview dots draw at.
    preview_level: u8,
    preview_scars: bool,
    /// Condition suggestions for the shared builder.
    suggestions: HashMap<&'static str, Vec<String>>,
    /// Variant picker is open (choosing which pool doll to add).
    adding_variant: bool,
    error: Option<String>,
}

impl DollSetsEditorState {
    /// Re-scan the pool: art added or renamed since the window opened
    /// appears, and every doll re-reads its sidecar and layer set.
    ///
    /// Selections are kept by NAME, not index, because a new file shifts
    /// every index in the list.
    fn rescan(&mut self) {
        let base_name = self.base.map(|i| self.entries[i].label.clone());
        let variant_names: Vec<String> = self
            .variants
            .iter()
            .map(|&i| self.entries[i].label.clone())
            .collect();
        let editing_name = self
            .editing
            .and_then(|slot| variant_names.get(slot).cloned());

        pool::invalidate_cache();
        self.entries = Self::scan();
        let find = |name: &str, entries: &[DollEntry]| {
            entries.iter().position(|e| e.label.eq_ignore_ascii_case(name))
        };
        self.base = base_name
            .as_deref()
            .and_then(|name| find(name, &self.entries))
            .or_else(|| self.entries.iter().position(|e| e.when.is_none()))
            .or(if self.entries.is_empty() { None } else { Some(0) });
        self.variants = variant_names
            .iter()
            .filter_map(|name| find(name, &self.entries))
            .filter(|i| Some(*i) != self.base)
            .collect();
        self.editing = editing_name
            .as_deref()
            .and_then(|name| find(name, &self.entries))
            .and_then(|index| self.variants.iter().position(|&v| v == index));
        self.error = None;
    }

    /// Every pool doll, layer files excluded.
    fn scan() -> Vec<DollEntry> {
        pool::list_category("dolls")
            .iter()
            .filter(|image| !pool::is_doll_layer_file(&image.pool_path))
            .map(DollEntry::load)
            .collect()
    }

    pub(crate) fn open(
        suggestions: HashMap<&'static str, Vec<String>>,
    ) -> (Option<Self>, CalibrationOutcome) {
        let mut outcome = CalibrationOutcome::default();
        let images = pool::list_category("dolls");
        if images.is_empty() {
            outcome.messages.push(
                "No doll images in the pool (global/images/dolls/). Drop PNGs there or \
                 install some with .jinx, then open the workbench."
                    .to_owned(),
            );
            return (None, outcome);
        }
        // A layer file is not itself a doll, or the picker is forty rows
        // of wound art.
        let entries: Vec<DollEntry> = Self::scan();
        if entries.is_empty() {
            outcome
                .messages
                .push("Every doll image looks like a layer file; nothing to edit.".to_owned());
            return (None, outcome);
        }
        // Open on the first doll that carries no rule (a plain base), so
        // the workbench lands somewhere sensible.
        let base = entries
            .iter()
            .position(|entry| entry.when.is_none())
            .or(Some(0));
        let variants = base
            .map(|base_index| {
                (0..entries.len())
                    .filter(|&i| i != base_index && entries[i].when.is_some())
                    .collect()
            })
            .unwrap_or_default();
        (
            Some(DollSetsEditorState {
                entries,
                base,
                variants,
                editing: None,
                selected_part: 0,
                auto_advance: true,
                preview_level: 2,
                preview_scars: false,
                suggestions,
                adding_variant: false,
                error: None,
            }),
            outcome,
        )
    }

    /// Index into `entries` of the doll currently being edited.
    fn current(&self) -> Option<usize> {
        match self.editing {
            None => self.base,
            Some(slot) => self.variants.get(slot).copied(),
        }
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context) -> CalibrationOutcome {
        let mut outcome = CalibrationOutcome::default();
        if let Some(index) = self.current() {
            self.entries[index].ensure_textures(ctx);
        }
        let mut open = true;
        let mut save_request = false;
        let mut rescan_request = false;
        let mut base_request: Option<usize> = None;
        let mut add_variant: Option<usize> = None;
        let mut drop_variant: Option<usize> = None;

        egui::Window::new("Injury Doll")
            .id(egui::Id::new("gui_doll_sets"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(900.0)
            .default_height(640.0)
            .min_width(MIN_CANVAS_WIDTH + SIDE_PANEL_WIDTH + 32.0)
            .min_height(360.0)
            .resizable(true)
            .show(ctx, |ui| {
                // ---- Base doll picker -------------------------------
                ui.horizontal(|ui| {
                    ui.label("Base doll");
                    let label = self
                        .base
                        .map(|i| self.entries[i].label.clone())
                        .unwrap_or_else(|| "(pick)".to_owned());
                    egui::ComboBox::from_id_salt("doll_wb_base")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (index, entry) in self.entries.iter().enumerate() {
                                if ui
                                    .selectable_label(self.base == Some(index), &entry.label)
                                    .clicked()
                                {
                                    base_request = Some(index);
                                }
                            }
                        });
                    if self.entries.iter().any(|e| e.dirty) && ui.button("Save").clicked() {
                        save_request = true;
                    }
                    if ui
                        .button("Refresh")
                        .on_hover_text(
                            "Re-scan the doll pool — picks up art added or renamed since                              this window opened",
                        )
                        .clicked()
                    {
                        rescan_request = true;
                    }
                });

                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }

                // ---- Base / variant tabs ----------------------------
                ui.horizontal_wrapped(|ui| {
                    let base_label = self
                        .base
                        .map(|i| format!("Base: {}", self.entries[i].label))
                        .unwrap_or_else(|| "Base".to_owned());
                    if ui.selectable_label(self.editing.is_none(), base_label).clicked() {
                        self.editing = None;
                    }
                    let slots: Vec<(usize, String, bool)> = self
                        .variants
                        .iter()
                        .enumerate()
                        .map(|(slot, &index)| {
                            (slot, self.entries[index].label.clone(), self.editing == Some(slot))
                        })
                        .collect();
                    for (slot, label, selected) in slots {
                        if ui.selectable_label(selected, &label).clicked() {
                            self.editing = Some(slot);
                        }
                    }
                    if ui
                        .button("+ Variant")
                        .on_hover_text(
                            "Add another doll as a variant of this base. It gets the same \
                             tooling — its own art, anchors, layers and rule.",
                        )
                        .clicked()
                    {
                        self.adding_variant = !self.adding_variant;
                    }
                    if self.editing.is_some()
                        && ui
                            .button("− Remove")
                            .on_hover_text("Stop treating this doll as a variant of the base")
                            .clicked()
                    {
                        drop_variant = self.editing;
                    }
                });

                if self.adding_variant {
                    ui.group(|ui| {
                        ui.label("Add which doll as a variant?");
                        let candidates: Vec<(usize, String)> = (0..self.entries.len())
                            .filter(|i| Some(*i) != self.base && !self.variants.contains(i))
                            .map(|i| (i, self.entries[i].label.clone()))
                            .collect();
                        if candidates.is_empty() {
                            ui.weak("Every pool doll is already in this set.");
                        }
                        for (index, label) in candidates {
                            if ui.selectable_label(false, label).clicked() {
                                add_variant = Some(index);
                            }
                        }
                    });
                }

                let Some(index) = self.current() else {
                    return;
                };
                ui.separator();

                // ---- Canvas + side panel ----------------------------
                //
                // Measure the SPLIT BEFORE entering the horizontal layout.
                // Inside one, `available_width` reports the unbounded room
                // a horizontal run may claim, so sizing a child from it
                // feeds its own width back in and the window grows without
                // limit on every frame.
                let outer = ui.available_width();
                let side_w = SIDE_PANEL_WIDTH.min((outer - MIN_CANVAS_WIDTH).max(0.0));
                let canvas_w = (outer - side_w - ui.spacing().item_spacing.x)
                    .max(MIN_CANVAS_WIDTH);
                // The rule editor sits BELOW the split at full width: its
                // condition rows (kind + operand dropdowns + text) need far
                // more than the controls column, and squeezing them into it
                // is what clipped the panel.
                let rule_h = if self.editing.is_some() { RULE_STRIP_HEIGHT } else { 0.0 };
                let canvas_h = (ui.available_height() - rule_h).max(MIN_CANVAS_HEIGHT);
                ui.horizontal_top(|ui| {
                    let canvas = egui::vec2(canvas_w, canvas_h);
                    self.canvas_ui(ui, index, canvas);
                    // A hard ceiling, not just a floor: `set_width` alone
                    // is a MINIMUM, so one over-wide child (the condition
                    // builder's dropdown row) would still push the column
                    // out and drag the window with it. `set_max_width` and
                    // a horizontal scrollbar keep that contained.
                    ui.allocate_ui_with_layout(
                        egui::vec2(side_w, canvas_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_width(side_w);
                            egui::ScrollArea::both()
                                .id_salt("doll_wb_side")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_max_width(side_w - 12.0);
                                    self.layers_ui(ui, index);
                                    ui.add_space(6.0);
                                    self.anchors_ui(ui, index);
                                    ui.add_space(6.0);
                                    self.dots_ui(ui, index);
                                });
                        },
                    );
                });
                ui.separator();
                if self.editing.is_some() {
                    // Full width: condition rows are far wider than the
                    // controls column, and horizontal scroll covers the
                    // deepest nesting rather than clipping it.
                    egui::ScrollArea::horizontal()
                        .id_salt("doll_wb_rule")
                        .show(ui, |ui| {
                            self.rule_ui(ui, index);
                        });
                } else {
                    ui.weak("Base doll — renders whenever no variant matches.");
                }
            });

        if let Some(index) = base_request {
            self.set_base(index);
        }
        if let Some(index) = add_variant {
            self.attach_variant(index);
            self.adding_variant = false;
        }
        if let Some(slot) = drop_variant {
            self.detach_variant(slot);
        }
        if save_request {
            self.save_all(&mut outcome);
        }
        if rescan_request {
            self.rescan();
        }
        if !open {
            outcome.closed = true;
        }
        outcome
    }

    /// The preview: base art, every enabled layer over it, then dots for
    /// the parts that have no art. Click places the selected part's anchor.
    fn canvas_ui(&mut self, ui: &mut egui::Ui, index: usize, canvas: egui::Vec2) {
        let (rect, response) = ui.allocate_exact_size(canvas, egui::Sense::click());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

        let Some(base_handle) = self.entries[index].base.as_ref() else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "no image",
                egui::FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        let base = SkinTexture {
            texture: base_handle.id(),
            size: base_handle.size_vec2(),
        };
        // Every layer shares the base's canvas, so one dest rect aligns
        // them all — this is why sprite_dest is computed from the base.
        let dest = gui_skin::sprite_dest(&base, rect);
        gui_skin::paint_sprite(&painter, dest, &base, egui::Color32::WHITE);
        for layer in &self.entries[index].layers {
            if !layer.shown {
                continue;
            }
            if let Some(handle) = &layer.texture {
                let sprite = SkinTexture {
                    texture: handle.id(),
                    size: handle.size_vec2(),
                };
                gui_skin::paint_sprite(&painter, dest, &sprite, egui::Color32::WHITE);
            }
        }

        // Dots stand in for parts with no art; a part that HAS art renders
        // hand-drawn, so showing a dot there would be a lie.
        let with_art = self.entries[index].parts_with_art();
        let style = ResolvedDotStyle::from_spec(&self.entries[index].dot_spec());
        let level = self.entries[index].preview_dot_level(self.preview_level, self.preview_scars);
        let radius = (style.diameter * dest.height() / 2.0).max(4.0);
        let at = |anchor: egui::Vec2| {
            dest.min + egui::Vec2::new(anchor.x * dest.width(), anchor.y * dest.height())
        };
        for (key, _, _) in DOLL_PARTS {
            if with_art.contains(&key.to_ascii_lowercase()) {
                continue;
            }
            gui_skin::paint_severity_dot(
                &painter,
                at(self.entries[index].anchor_for(key)),
                radius,
                level,
                &style,
            );
        }

        // Crosshair on the part being placed.
        if let Some((key, _, _)) = DOLL_PARTS.get(self.selected_part) {
            let center = at(self.entries[index].anchor_for(key));
            let color = ui.visuals().hyperlink_color;
            painter.line_segment(
                [
                    egui::pos2(dest.min.x, center.y),
                    egui::pos2(dest.max.x, center.y),
                ],
                egui::Stroke::new(1.0, color),
            );
            painter.line_segment(
                [
                    egui::pos2(center.x, dest.min.y),
                    egui::pos2(center.x, dest.max.y),
                ],
                egui::Stroke::new(1.0, color),
            );
            painter.circle_stroke(center, radius + 3.0, egui::Stroke::new(2.0, color));
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if dest.contains(pos) && dest.width() > 0.0 && dest.height() > 0.0 {
                    let normalized = [
                        ((pos.x - dest.min.x) / dest.width()).clamp(0.0, 1.0),
                        ((pos.y - dest.min.y) / dest.height()).clamp(0.0, 1.0),
                    ];
                    let key = DOLL_PARTS[self.selected_part].0.to_ascii_lowercase();
                    self.entries[index].anchors.insert(key, normalized);
                    self.entries[index].dirty = true;
                    if self.auto_advance {
                        self.selected_part = self.next_dotted_part(index, self.selected_part);
                    }
                }
            }
        }
    }

    /// Layer toggles: what art this doll has, and what each one looks like.
    fn layers_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        ui.strong("Wound layers");
        // Whose layers these are: a doll that draws its own owns them,
        // otherwise it shares its group's. Shown because the sharing is
        // invisible in the file names alone.
        let owner = self.entries[index].layer_owner.clone();
        let own = owner
            .as_deref()
            .is_some_and(|o| o.eq_ignore_ascii_case(&self.entries[index].label));
        if let Some(owner) = &owner {
            if own {
                ui.weak("This doll's own layers.");
            } else {
                ui.weak(format!("Shared from {owner}."))
                    .on_hover_text(
                        "Dolls sharing a name prefix share one layer set. Give this doll a                          layer of its own and it stops sharing — parts you leave undrawn                          fall back to dots.",
                    );
            }
        }
        if self.entries[index].layers.is_empty() {
            // Name the file this doll WOULD pick up, so the message is
            // actionable rather than just a complaint.
            let example = owner
                .as_deref()
                .and_then(|owner| {
                    pool::doll_layer_file_name(&format!("dolls/{owner}.png"), "chest", 2)
                })
                .unwrap_or_default();
            ui.weak("No layer art yet — every part renders as a dot.");
            ui.weak(format!("Add files named like {example}, then Refresh."));
            return;
        }
        ui.horizontal(|ui| {
            if ui.small_button("All").clicked() {
                for layer in &mut self.entries[index].layers {
                    layer.shown = true;
                }
            }
            if ui.small_button("None").clicked() {
                for layer in &mut self.entries[index].layers {
                    layer.shown = false;
                }
            }
        });
        for layer in &mut self.entries[index].layers {
            let label = layer.label();
            let missing = layer.texture.is_none();
            ui.horizontal(|ui| {
                ui.checkbox(&mut layer.shown, label);
                if missing {
                    ui.colored_label(egui::Color32::LIGHT_RED, "!")
                        .on_hover_text("Image could not be loaded");
                }
            });
        }
    }

    /// Anchor placement for the parts that fall back to generated dots.
    fn anchors_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        ui.strong("Dot anchors");
        let with_art = self.entries[index].parts_with_art();
        ui.weak("Click the image to place the highlighted part.");
        ui.checkbox(&mut self.auto_advance, "Advance after each click");
        for (slot, (key, label, _)) in DOLL_PARTS.iter().enumerate() {
            let has_art = with_art.contains(&key.to_ascii_lowercase());
            let placed = self.entries[index]
                .anchors
                .contains_key(&key.to_ascii_lowercase());
            let text = if has_art {
                format!("{label}  (hand-drawn)")
            } else if placed {
                format!("{label}  ✓")
            } else {
                (*label).to_owned()
            };
            let mut row = ui.selectable_label(self.selected_part == slot, text);
            if has_art {
                row = row.on_hover_text(
                    "This part has layer art, so it renders hand-drawn and ignores its dot \
                     anchor.",
                );
            }
            if row.clicked() {
                self.selected_part = slot;
            }
        }
    }

    fn dots_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        ui.strong("Dot style");
        let entry = &mut self.entries[index];
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Wound");
            let before = entry.wound_color.clone();
            super::color_field(ui, &mut entry.wound_color);
            changed |= before != entry.wound_color;
        });
        ui.horizontal(|ui| {
            ui.label("Scar");
            let before = entry.scar_color.clone();
            super::color_field(ui, &mut entry.scar_color);
            changed |= before != entry.scar_color;
        });
        ui.horizontal(|ui| {
            ui.label("Size");
            changed |= ui
                .add(egui::DragValue::new(&mut entry.diameter).speed(0.002).range(0.01..=0.5))
                .changed();
            ui.label("Opacity");
            changed |= ui
                .add(egui::DragValue::new(&mut entry.opacity).speed(0.02).range(0.0..=1.0))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Preview");
            for level in 1..=3u8 {
                if ui
                    .selectable_label(self.preview_level == level, format!("{level}"))
                    .clicked()
                {
                    self.preview_level = level;
                }
            }
            ui.checkbox(&mut self.preview_scars, "scars");
        });
        if changed {
            entry.dirty = true;
        }
    }

    /// The rule that selects this doll. The base has none by definition —
    /// it is what renders when nothing matches.
    fn rule_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        ui.strong("When to use this doll");
        let mut enabled = self.entries[index].when.is_some();
        if ui
            .checkbox(&mut enabled, "Select by game state")
            .on_hover_text("Off: this doll is only reachable by name.")
            .changed()
        {
            self.entries[index].when = enabled.then(|| Condition::All {
                conditions: Vec::new(),
            });
            self.entries[index].dirty = true;
        }
        if self.entries[index].when.is_none() {
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Priority");
            if ui
                .add(
                    egui::DragValue::new(&mut self.entries[index].priority)
                        .speed(1.0)
                        .range(-99..=99),
                )
                .on_hover_text("Lower is checked first. Specific rules need lower numbers.")
                .changed()
            {
                self.entries[index].dirty = true;
            }
        });
        let suggestions = self.suggestions.clone();
        if let Some(cond) = self.entries[index].when.as_mut() {
            if super::hotbars::render_condition_group(
                ui,
                &format!("doll_wb_rule_{index}"),
                cond,
                0,
                &suggestions,
            ) {
                self.entries[index].dirty = true;
            }
        }
    }

    /// Next part that still takes a dot, so auto-advance skips the parts
    /// whose art makes an anchor meaningless.
    fn next_dotted_part(&self, index: usize, from: usize) -> usize {
        let with_art = self.entries[index].parts_with_art();
        for step in 1..=DOLL_PARTS.len() {
            let slot = (from + step) % DOLL_PARTS.len();
            if !with_art.contains(&DOLL_PARTS[slot].0.to_ascii_lowercase()) {
                return slot;
            }
        }
        from
    }

    fn set_base(&mut self, index: usize) {
        self.base = Some(index);
        self.editing = None;
        self.variants.retain(|&i| i != index);
        self.selected_part = 0;
    }

    /// Adding a variant copies the base's calibration as a starting point:
    /// a new pose still needs every anchor moved, but starting from the
    /// base's placement beats starting from nothing.
    fn attach_variant(&mut self, index: usize) {
        let Some(base_index) = self.base else {
            return;
        };
        if self.entries[index].anchors.is_empty() {
            let inherited = self.entries[base_index].anchors.clone();
            let spec = self.entries[base_index].dot_spec();
            let entry = &mut self.entries[index];
            entry.anchors = inherited;
            entry.wound_color = spec.wound_color;
            entry.scar_color = spec.scar_color;
            entry.opacity = spec.opacity;
            entry.diameter = spec.diameter;
            entry.dirty = true;
        }
        if self.entries[index].when.is_none() {
            self.entries[index].when = Some(Condition::All {
                conditions: Vec::new(),
            });
            self.entries[index].dirty = true;
        }
        self.variants.push(index);
        self.editing = Some(self.variants.len() - 1);
    }

    fn detach_variant(&mut self, slot: usize) {
        if slot >= self.variants.len() {
            return;
        }
        let index = self.variants.remove(slot);
        // Dropping the rule is what actually un-variants it; the art stays
        // in the pool, selectable by name.
        self.entries[index].when = None;
        self.entries[index].dirty = true;
        self.editing = None;
    }

    fn save_all(&mut self, outcome: &mut CalibrationOutcome) {
        let mut saved = 0usize;
        for entry in &mut self.entries {
            if !entry.dirty {
                continue;
            }
            match entry.save() {
                Ok(()) => saved += 1,
                Err(err) => {
                    self.error = Some(format!("Cannot save {}: {err}", entry.label));
                    return;
                }
            }
        }
        if saved > 0 {
            self.error = None;
            outcome.messages.push(format!("Injury doll: saved {saved} doll(s)"));
            outcome.reload_art = true;
        }
    }
}

impl DollEntry {
    /// Dot severity for the preview: 1-3 wounds, 4-6 scars.
    fn preview_dot_level(&self, level: u8, scars: bool) -> u8 {
        level.clamp(1, 3) + if scars { 3 } else { 0 }
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_doll_sets(&mut self) {
        if self.doll_sets_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_doll_sets"));
            return;
        }
        let suggestions = self.condition_suggestions();
        let (state, outcome) = DollSetsEditorState::open(suggestions);
        self.doll_sets_editor = state;
        for message in outcome.messages {
            self.app_core.add_system_message(&message);
        }
    }

    pub(in super::super) fn render_doll_sets(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.doll_sets_editor.take() else {
            return;
        };
        let outcome = state.ui(ctx);
        for message in outcome.messages {
            self.app_core.add_system_message(&message);
        }
        if outcome.reload_art {
            self.skin_state.force_reload();
        }
        if !outcome.closed {
            self.doll_sets_editor = Some(state);
        }
    }
}
