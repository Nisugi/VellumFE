//! `.roomimages edit` — room art mappings, grouped by image.
//!
//! One card per picture, because a single image usually covers many rooms
//! (a shop interior, a stretch of forest, every pier on a dock). The
//! primary authoring path is "stand in the room and press Add current
//! room": the uid comes from the game, so the user never types one.

use crate::config::room_images::{RoomImageDef, RoomImagesConfig, DEFAULT_ROOM_IMAGE_ROWS};
use crate::core::elanthian_time::DayPhase;
use crate::data::FloatAlign;
use crate::frontend::gui::app::VellumGuiApp;

pub(in super::super) struct RoomImagesEditorState {
    draft: RoomImagesConfig,
    /// Pool image names available to assign.
    available: Vec<String>,
    /// Name for the "new mapping" picker.
    new_image: String,
}

/// Deferred mutation so the render loop never edits what it iterates.
enum Op {
    RemoveRoom { image: usize, room: usize },
    RemoveImage(usize),
    AddCurrentRoom(usize),
    AddVariant(usize),
    RemoveVariant { image: usize, variant: usize },
}

impl VellumGuiApp {
    pub(in super::super) fn open_room_images_editor(&mut self) {
        if self.room_images_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_room_images_editor"));
            return;
        }
        let draft = self.app_core.room_images_store().clone();
        let mut available: Vec<String> = crate::core::inline_image::all()
            .into_iter()
            .map(|art| art.name)
            .collect();
        available.sort();
        let new_image = available.first().cloned().unwrap_or_default();
        self.room_images_editor = Some(RoomImagesEditorState {
            draft,
            available,
            new_image,
        });
    }

    pub(in super::super) fn render_room_images_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.room_images_editor.take() else {
            return;
        };
        let mut open = true;
        let mut saved = false;
        let mut cancelled = false;
        let mut op: Option<Op> = None;

        let current_uid = self.app_core.message_processor.current_room_uid();
        let current_name = self
            .app_core
            .game_state
            .room_name
            .clone()
            .or_else(|| self.app_core.room_subtitle.clone());
        let mut enabled = self.app_core.config.room_images.enabled;

        egui::Window::new("Room Images")
            .id(egui::Id::new("gui_room_images_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.checkbox(&mut enabled, "Show room images (.roomimages)")
                    .on_hover_text(
                        "Fills the room window's art slot when you enter a mapped room",
                    );
                if state.available.is_empty() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "No images installed. Put art in \
                         ~/.vellum-fe/global/images/inline/ and run .reload.",
                    );
                }
                ui.label(match (&current_uid, &current_name) {
                    (Some(uid), Some(name)) => format!("You are in: {name} ({uid})"),
                    (Some(uid), None) => format!("You are in room {uid}"),
                    (None, _) => "Room unknown — move once to enable 'Add current room'."
                        .to_string(),
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        for (image_idx, entry) in state.draft.images.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    // Thumbnail, so a card is identifiable at a glance.
                                    if let Some((tex, size)) = self
                                        .skin_state
                                        .thumbnail(ctx, &format!("inline/{}.png", entry.name))
                                    {
                                        let scale = 40.0 / size.y.max(1.0);
                                        ui.image((tex, size * scale));
                                    }
                                    ui.vertical(|ui| {
                                        ui.strong(&entry.name);
                                        ui.horizontal(|ui| {
                                            let mut rows = entry.rows_or_default();
                                            ui.label("Rows:");
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut rows)
                                                        .range(1.0..=8.0)
                                                        .speed(0.25),
                                                )
                                                .changed()
                                            {
                                                entry.rows = Some(rows);
                                            }
                                            ui.label("Align:");
                                            let mut align = entry.align_or_default();
                                            egui::ComboBox::from_id_salt((
                                                "room_img_align",
                                                image_idx,
                                            ))
                                            .selected_text(match align {
                                                FloatAlign::Left => "Left",
                                                FloatAlign::Right => "Right",
                                            })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut align,
                                                    FloatAlign::Left,
                                                    "Left",
                                                );
                                                ui.selectable_value(
                                                    &mut align,
                                                    FloatAlign::Right,
                                                    "Right",
                                                );
                                            });
                                            if align != entry.align_or_default() {
                                                entry.align = Some(align);
                                            }
                                        });
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .button("✕")
                                                .on_hover_text("Remove this mapping")
                                                .clicked()
                                            {
                                                op = Some(Op::RemoveImage(image_idx));
                                            }
                                        },
                                    );
                                });

                                if entry.rooms.is_empty() {
                                    ui.weak("No rooms yet.");
                                }
                                for (room_idx, room) in entry.rooms.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        let label = state
                                            .draft
                                            .names
                                            .get(&room.to_string())
                                            .cloned()
                                            .unwrap_or_default();
                                        ui.monospace(format!("{room}"));
                                        if !label.is_empty() {
                                            ui.label(label);
                                        }
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button("✕")
                                                    .on_hover_text("Unmap this room")
                                                    .clicked()
                                                {
                                                    op = Some(Op::RemoveRoom {
                                                        image: image_idx,
                                                        room: room_idx,
                                                    });
                                                }
                                            },
                                        );
                                    });
                                }

                                let already_here = current_uid
                                    .is_some_and(|uid| entry.rooms.contains(&uid));
                                ui.add_enabled_ui(
                                    current_uid.is_some() && !already_here,
                                    |ui| {
                                        if ui.button("+ Add current room").clicked() {
                                            op = Some(Op::AddCurrentRoom(image_idx));
                                        }
                                    },
                                );
                                if already_here {
                                    ui.weak("This room is already mapped here.");
                                }

                                // Conditional art: a night version of the
                                // same view, a wounded version, and so on.
                                // First match wins, which is why order is
                                // shown and adjustable.
                                ui.separator();
                                if entry.variants.is_empty() {
                                    ui.weak("No variants — this art always shows.");
                                }
                                for (variant_idx, variant) in
                                    entry.variants.iter_mut().enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        ui.label("when");
                                        // Time of day is the case this was
                                        // built for; other conditions are
                                        // editable in the file and shown
                                        // read-only here rather than
                                        // silently dropped.
                                        match &mut variant.when {
                                            crate::config::Condition::TimeOfDay { phase } => {
                                                egui::ComboBox::from_id_salt((
                                                    "room_img_variant_phase",
                                                    image_idx,
                                                    variant_idx,
                                                ))
                                                .selected_text(phase.as_str())
                                                .show_ui(ui, |ui| {
                                                    for option in [
                                                        DayPhase::Dawn,
                                                        DayPhase::Day,
                                                        DayPhase::Dusk,
                                                        DayPhase::Night,
                                                    ] {
                                                        ui.selectable_value(
                                                            phase,
                                                            option,
                                                            option.as_str(),
                                                        );
                                                    }
                                                });
                                            }
                                            other => {
                                                ui.weak(format!("{other:?}"))
                                                    .on_hover_text(
                                                        "Edit this condition in                                                          room_images.toml",
                                                    );
                                            }
                                        }
                                        ui.label("show");
                                        egui::ComboBox::from_id_salt((
                                            "room_img_variant_art",
                                            image_idx,
                                            variant_idx,
                                        ))
                                        .selected_text(if variant.name.is_empty() {
                                            "(pick art)".to_string()
                                        } else {
                                            variant.name.clone()
                                        })
                                        .show_ui(ui, |ui| {
                                            for art in &state.available {
                                                ui.selectable_value(
                                                    &mut variant.name,
                                                    art.clone(),
                                                    art,
                                                );
                                            }
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button("✕")
                                                    .on_hover_text("Remove this variant")
                                                    .clicked()
                                                {
                                                    op = Some(Op::RemoveVariant {
                                                        image: image_idx,
                                                        variant: variant_idx,
                                                    });
                                                }
                                            },
                                        );
                                    });
                                }
                                ui.add_enabled_ui(!state.available.is_empty(), |ui| {
                                    if ui
                                        .button("+ Add variant")
                                        .on_hover_text(
                                            "Show different art when a condition matches                                              (e.g. at night)",
                                        )
                                        .clicked()
                                    {
                                        op = Some(Op::AddVariant(image_idx));
                                    }
                                });
                            });
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("room_img_new")
                        .selected_text(if state.new_image.is_empty() {
                            "(pick art)".to_string()
                        } else {
                            state.new_image.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &state.available {
                                ui.selectable_value(
                                    &mut state.new_image,
                                    name.clone(),
                                    name,
                                );
                            }
                        });
                    let exists = state
                        .draft
                        .images
                        .iter()
                        .any(|i| i.name == state.new_image);
                    ui.add_enabled_ui(!state.new_image.is_empty() && !exists, |ui| {
                        if ui.button("+ New image mapping").clicked() {
                            state.draft.images.push(RoomImageDef {
                                name: state.new_image.clone(),
                                rooms: Vec::new(),
                                rows: Some(DEFAULT_ROOM_IMAGE_ROWS),
                                align: None,
                                variants: Vec::new(),
                            });
                        }
                    });
                    if exists && !state.new_image.is_empty() {
                        ui.weak("Already has a card.");
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        saved = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });

        // Apply the deferred mutation now that the iteration is finished.
        match op {
            Some(Op::RemoveImage(idx)) => {
                if idx < state.draft.images.len() {
                    state.draft.images.remove(idx);
                }
            }
            Some(Op::RemoveRoom { image, room }) => {
                if let Some(entry) = state.draft.images.get_mut(image) {
                    if room < entry.rooms.len() {
                        entry.rooms.remove(room);
                    }
                }
            }
            Some(Op::AddCurrentRoom(idx)) => {
                if let Some(uid) = current_uid {
                    // A room belongs to exactly one image: drop it elsewhere
                    // first so adding MOVES rather than duplicating.
                    for (other, entry) in state.draft.images.iter_mut().enumerate() {
                        if other != idx {
                            entry.rooms.retain(|r| *r != uid);
                        }
                    }
                    if let Some(entry) = state.draft.images.get_mut(idx) {
                        if !entry.rooms.contains(&uid) {
                            entry.rooms.push(uid);
                        }
                    }
                    if let Some(name) = current_name.clone() {
                        state.draft.names.insert(uid.to_string(), name);
                    }
                }
            }
            Some(Op::AddVariant(idx)) => {
                if let Some(entry) = state.draft.images.get_mut(idx) {
                    // Default to a night variant: that is the case this
                    // exists for, and it is one combo change away from any
                    // other phase.
                    entry.variants.push(crate::config::room_images::RoomImageVariant {
                        name: state.new_image.clone(),
                        when: crate::config::Condition::TimeOfDay {
                            phase: DayPhase::Night,
                        },
                    });
                }
            }
            Some(Op::RemoveVariant { image, variant }) => {
                if let Some(entry) = state.draft.images.get_mut(image) {
                    if variant < entry.variants.len() {
                        entry.variants.remove(variant);
                    }
                }
            }
            None => {}
        }

        if saved {
            if enabled != self.app_core.config.room_images.enabled {
                self.app_core.config.room_images.enabled = enabled;
                self.app_core
                    .message_processor
                    .set_room_images_enabled(enabled);
                if let Err(err) = self.app_core.save_config() {
                    self.app_core
                        .add_system_message(&format!("Toggle saved to session only: {err}"));
                }
            }
            self.app_core.commit_room_images(state.draft.clone());
            self.app_core.add_system_message("Room images saved.");
        }
        if !saved && !cancelled && open {
            self.room_images_editor = Some(state);
        }
    }
}
