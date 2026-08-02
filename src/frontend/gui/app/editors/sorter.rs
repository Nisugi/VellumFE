//! `.sorter edit` — the categorized container-look editor: user rules,
//! category order, display renames, and formatting, with a live preview
//! fed through the REAL transform on every draft change (the transform is
//! a pure function, so the preview is always truthful).

use crate::config::{SorterConfig, SorterRule};
use crate::core::gameobj_data::GameObjData;
use crate::data::widget::{LinkData, SpanType, TextSegment};
use crate::frontend::gui::app::VellumGuiApp;

pub(in super::super) struct SorterEditorState {
    draft: SorterConfig,
}

/// Deferred list mutation so the render loop never edits what it iterates.
enum RuleOp {
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
}

/// Canned container look for the preview: the same shape the game sends,
/// classified by the real data pack so the preview shows real categories.
fn preview_line() -> (Vec<TextSegment>, String) {
    fn link(text: &str, id: &str, noun: &str) -> TextSegment {
        TextSegment {
            text: text.to_string(),
            link_data: Some(LinkData {
                exist_id: id.to_string(),
                noun: noun.to_string(),
                text: text.to_string(),
                coord: None,
            }),
            ..Default::default()
        }
    }
    let segments = vec![
        TextSegment::plain("In the "),
        link("backpack", "1", "backpack"),
        TextSegment::plain(" you see a "),
        link("blue sapphire", "2", "sapphire"),
        TextSegment::plain(", a "),
        link("quartz crystal", "3", "crystal"),
        TextSegment::plain(", a "),
        link("blue sapphire", "4", "sapphire"),
        TextSegment::plain(", a "),
        link("copper lockpick", "5", "lockpick"),
        TextSegment::plain(", some "),
        link("acantha leaf", "6", "leaf"),
        TextSegment::plain(" and a "),
        link("crystal wand", "7", "wand"),
        TextSegment::plain("."),
    ];
    let text: String = segments.iter().map(|s| s.text.as_str()).collect();
    (segments, text)
}

impl VellumGuiApp {
    pub(in super::super) fn open_sorter_editor(&mut self) {
        if self.sorter_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_sorter_editor"));
            return;
        }
        self.sorter_editor = Some(SorterEditorState {
            draft: self.app_core.config.sorter.clone(),
        });
    }

    pub(in super::super) fn render_sorter_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.sorter_editor.take() else {
            return;
        };
        let mut open = true;
        let mut saved = false;
        let mut cancelled = false;

        // Categories the editor lists: the data pack's tags (file order),
        // the catch-all, then anything else the draft references.
        let mut known: Vec<String> = self
            .app_core
            .gameobj_data_cached()
            .map(|data| {
                data.known_categories()
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if !known.iter().any(|c| c == "other") {
            known.push("other".to_string());
        }
        let extras: Vec<String> = state
            .draft
            .category_order
            .iter()
            .chain(state.draft.labels.keys())
            .chain(state.draft.rules.iter().map(|rule| &rule.category))
            .filter(|c| !c.trim().is_empty())
            .cloned()
            .collect();
        for extra in extras {
            if !known.iter().any(|c| c.eq_ignore_ascii_case(&extra)) {
                known.push(extra);
            }
        }
        // Effective display order: explicit order first, rest as listed.
        let order_position = |order: &[String], category: &str| {
            order
                .iter()
                .position(|entry| entry.eq_ignore_ascii_case(category))
                .unwrap_or(usize::MAX)
        };
        let mut ordered = known;
        {
            let order = state.draft.category_order.clone();
            ordered.sort_by_key(|category| order_position(&order, category));
        }

        let data_pack = self.app_core.gameobj_data_cached();
        let empty_pack;
        let preview_pack: &GameObjData = match data_pack {
            Some(data) => data,
            None => {
                empty_pack = GameObjData::parse("<data></data>");
                &empty_pack
            }
        };

        egui::Window::new("Sorter")
            .id(egui::Id::new("gui_sorter_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(460.0)
            .show(ctx, |ui| {
                let draft = &mut state.draft;
                ui.checkbox(
                    &mut draft.enabled,
                    "Sort container looks (.sorter)",
                );
                ui.horizontal(|ui| {
                    ui.checkbox(&mut draft.show_counts, "Counts")
                        .on_hover_text("Duplicate counts and category totals");
                    ui.checkbox(&mut draft.bold_labels, "Bold labels");
                    ui.label("Item order:");
                    egui::ComboBox::from_id_salt("sorter_item_sort")
                        .selected_text(match draft.item_sort.as_str() {
                            "alpha" => "Alphabetical",
                            "none" => "As looked",
                            _ => "Last word",
                        })
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                ("last_word", "Last word (sorter.lic)"),
                                ("alpha", "Alphabetical"),
                                ("none", "As looked"),
                            ] {
                                if ui
                                    .selectable_label(draft.item_sort == value, label)
                                    .clicked()
                                {
                                    draft.item_sort = value.to_string();
                                }
                            }
                        });
                });
                ui.separator();

                egui::CollapsingHeader::new("Categories — order and names")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.weak(
                            "Top to bottom is display order. Rename shows a \
                             different label without changing the rules.",
                        );
                        let mut swap: Option<(usize, usize)> = None;
                        let last = ordered.len().saturating_sub(1);
                        egui::ScrollArea::vertical()
                            .id_salt("sorter_categories")
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for (idx, category) in ordered.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_enabled(idx > 0, egui::Button::new("⬆").small())
                                            .clicked()
                                        {
                                            swap = Some((idx, idx - 1));
                                        }
                                        if ui
                                            .add_enabled(
                                                idx < last,
                                                egui::Button::new("⬇").small(),
                                            )
                                            .clicked()
                                        {
                                            swap = Some((idx, idx + 1));
                                        }
                                        ui.label(category);
                                        let mut label = draft
                                            .labels
                                            .get(category)
                                            .cloned()
                                            .unwrap_or_default();
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut label)
                                                    .desired_width(120.0)
                                                    .hint_text("rename"),
                                            )
                                            .changed()
                                        {
                                            if label.trim().is_empty() {
                                                draft.labels.remove(category);
                                            } else {
                                                draft
                                                    .labels
                                                    .insert(category.clone(), label);
                                            }
                                        }
                                    });
                                }
                            });
                        if let Some((a, b)) = swap {
                            ordered.swap(a, b);
                            // Any reorder materializes the whole list as the
                            // explicit order — unambiguous and stable.
                            draft.category_order = ordered.clone();
                        }
                    });

                egui::CollapsingHeader::new("Rules")
                    .default_open(!draft.rules.is_empty())
                    .show(ui, |ui| {
                        ui.weak(
                            "Checked before the game data; first matching rule \
                             wins. Empty fields match anything.",
                        );
                        let mut op: Option<RuleOp> = None;
                        let last = draft.rules.len().saturating_sub(1);
                        for (idx, rule) in draft.rules.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut rule.name_match)
                                        .desired_width(110.0)
                                        .hint_text("name contains"),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut rule.noun)
                                        .desired_width(80.0)
                                        .hint_text("noun is"),
                                );
                                ui.label("→");
                                ui.add(
                                    egui::TextEdit::singleline(&mut rule.category)
                                        .desired_width(90.0)
                                        .hint_text("category"),
                                );
                                if ui
                                    .add_enabled(idx > 0, egui::Button::new("⬆").small())
                                    .clicked()
                                {
                                    op = Some(RuleOp::MoveUp(idx));
                                }
                                if ui
                                    .add_enabled(idx < last, egui::Button::new("⬇").small())
                                    .clicked()
                                {
                                    op = Some(RuleOp::MoveDown(idx));
                                }
                                if ui.small_button("✕").clicked() {
                                    op = Some(RuleOp::Remove(idx));
                                }
                            });
                        }
                        match op {
                            Some(RuleOp::MoveUp(idx)) => draft.rules.swap(idx, idx - 1),
                            Some(RuleOp::MoveDown(idx)) => draft.rules.swap(idx, idx + 1),
                            Some(RuleOp::Remove(idx)) => {
                                draft.rules.remove(idx);
                            }
                            None => {}
                        }
                        if ui.button("➕ Add rule").clicked() {
                            draft.rules.push(SorterRule {
                                name_match: String::new(),
                                noun: String::new(),
                                category: "other".to_string(),
                            });
                        }
                    });

                ui.separator();
                ui.label("Preview");
                if data_pack.is_none() {
                    ui.weak(
                        "Game data pack not loaded — preview classifies by \
                         your rules only.",
                    );
                }
                let (segments, text) = preview_line();
                match crate::core::sorter::transform(
                    &segments,
                    &text,
                    preview_pack,
                    draft,
                ) {
                    Some(lines) => {
                        for line in lines {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                for segment in &line {
                                    let mut rich =
                                        egui::RichText::new(&segment.text).monospace();
                                    if segment.span_type == SpanType::Monsterbold {
                                        rich = rich.strong();
                                    }
                                    if segment.link_data.is_some() {
                                        rich = rich.color(
                                            ui.visuals().hyperlink_color,
                                        );
                                    }
                                    ui.label(rich);
                                }
                            });
                        }
                    }
                    None => {
                        ui.monospace(&text);
                    }
                }

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

        if saved {
            self.app_core.config.sorter = state.draft.clone();
            self.app_core
                .message_processor
                .set_sorter_config(state.draft.clone());
            if let Err(err) = self.app_core.save_config() {
                self.app_core
                    .add_system_message(&format!("Sorter saved to session only: {err}"));
            } else {
                self.app_core.add_system_message("Sorter settings saved.");
            }
            self.sorter_editor = None;
        } else if cancelled || !open {
            self.sorter_editor = None;
        } else {
            self.sorter_editor = Some(state);
        }
    }
}
