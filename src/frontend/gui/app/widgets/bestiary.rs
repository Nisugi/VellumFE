//! Bestiary browser widget: the app-like GUI face of the bundled codex.
//!
//! Two pages, mirroring the design note: a browse page (search box,
//! level/family/undead filters, results table) and an entry page (stat
//! tiles, sections, back navigation). All data comes from
//! [`crate::core::bestiary::format::shared`]; view state is per-window
//! egui temp data so the renderer stays stateless like the other widgets.
//! Outbound actions (go2 jumps) ride the normal link-click pipeline as
//! `_direct_` dot-commands.

use super::*;
use crate::core::bestiary::{format as codex, CreatureEntry};

/// Per-window view state, cloned in and out of egui temp memory.
#[derive(Clone, Default)]
struct BestiaryViewState {
    query: String,
    level_lo: String,
    level_hi: String,
    family: String,
    undead_only: bool,
    /// Entry page when set (entry name); browse page when None.
    selected: Option<String>,
}

fn state_id(window: &str) -> egui::Id {
    egui::Id::new(("bestiary_view_state", window.to_string()))
}

impl VellumGuiApp {
    pub(in crate::frontend::gui::app) fn render_bestiary_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        window_name: &str,
    ) -> Option<GuiLinkClick> {
        let db = codex::shared();
        if db.is_empty() {
            ui.label("Bundled bestiary failed to load.");
            return None;
        }
        let sid = state_id(window_name);
        let mut state: BestiaryViewState = ui
            .ctx()
            .data_mut(|d| d.get_temp(sid))
            .unwrap_or_default();

        let clicked = if let Some(name) = state.selected.clone() {
            match db.resolve(&name) {
                Some(entry) => Self::bestiary_entry_page(app_core, ui, entry, &mut state),
                None => {
                    state.selected = None;
                    None
                }
            }
        } else {
            Self::bestiary_browse_page(ui, &mut state);
            None
        };

        ui.ctx().data_mut(|d| d.insert_temp(sid, state));
        clicked
    }

    fn bestiary_browse_page(ui: &mut egui::Ui, state: &mut BestiaryViewState) {
        let db = codex::shared();

        // ---- Toolbar ----------------------------------------------------
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text("Search creatures…")
                    .desired_width(160.0),
            );
            ui.label("Lvl");
            ui.add(
                egui::TextEdit::singleline(&mut state.level_lo)
                    .hint_text("min")
                    .desired_width(34.0),
            );
            ui.label("–");
            ui.add(
                egui::TextEdit::singleline(&mut state.level_hi)
                    .hint_text("max")
                    .desired_width(34.0),
            );
            let families: Vec<String> = {
                let mut f: Vec<String> = db
                    .entries
                    .iter()
                    .filter_map(|e| e.family.clone())
                    .collect();
                f.sort();
                f.dedup();
                f
            };
            egui::ComboBox::from_id_salt(("bestiary_family", state as *const _ as usize))
                .selected_text(if state.family.is_empty() {
                    "Family"
                } else {
                    state.family.as_str()
                })
                .width(110.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(state.family.is_empty(), "(any)").clicked() {
                        state.family.clear();
                    }
                    for f in families {
                        if ui.selectable_label(state.family == f, &f).clicked() {
                            state.family = f;
                        }
                    }
                });
            ui.toggle_value(&mut state.undead_only, "Undead");
        });
        ui.separator();

        // ---- Filter -----------------------------------------------------
        let q = state.query.trim().to_ascii_lowercase();
        let lo: Option<i64> = state.level_lo.trim().parse().ok();
        let hi: Option<i64> = state.level_hi.trim().parse().ok();
        let mut rows: Vec<&CreatureEntry> = db
            .entries
            .iter()
            .filter(|e| q.is_empty() || e.name.to_ascii_lowercase().contains(&q))
            .filter(|e| match (lo, hi) {
                (None, None) => true,
                _ => e.level.is_some_and(|l| {
                    lo.is_none_or(|lo| l >= lo) && hi.is_none_or(|hi| l <= hi)
                }),
            })
            .filter(|e| {
                state.family.is_empty() || e.family.as_deref() == Some(state.family.as_str())
            })
            .filter(|e| !state.undead_only || e.undead)
            .collect();
        rows.sort_by_key(|e| (e.level.unwrap_or(i64::MAX), e.name.clone()));

        ui.label(
            egui::RichText::new(format!("{} creatures", rows.len()))
                .small()
                .weak(),
        );

        // ---- Table ------------------------------------------------------
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("bestiary_rows")
                    .num_columns(4)
                    .striped(true)
                    .min_col_width(30.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Lvl").small().weak());
                        ui.label(egui::RichText::new("Name").small().weak());
                        ui.label(egui::RichText::new("Family").small().weak());
                        ui.label(egui::RichText::new("HP").small().weak());
                        ui.end_row();
                        for e in rows {
                            ui.label(
                                e.level.map(|l| l.to_string()).unwrap_or_else(|| "?".into()),
                            );
                            if ui.link(&e.name).clicked() {
                                state.selected = Some(e.name.clone());
                            }
                            if let Some(f) = &e.family {
                                if ui.link(f).clicked() {
                                    state.family = f.clone();
                                }
                            } else {
                                ui.label("");
                            }
                            ui.label(
                                e.max_hp.map(|h| h.to_string()).unwrap_or_default(),
                            );
                            ui.end_row();
                        }
                    });
            });
    }

    fn bestiary_entry_page(
        _app_core: &AppCore,
        ui: &mut egui::Ui,
        entry: &CreatureEntry,
        state: &mut BestiaryViewState,
    ) -> Option<GuiLinkClick> {
        let mut clicked: Option<GuiLinkClick> = None;
        ui.horizontal(|ui| {
            if ui.button("◂ Back").clicked() {
                state.selected = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(url) = &entry.url {
                    ui.hyperlink_to("wiki ↗", url);
                }
            });
        });
        ui.separator();

        // Header: monsterbold-red name + identity line.
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(entry.name.to_uppercase())
                    .strong()
                    .size(16.0)
                    .color(egui::Color32::from_rgb(228, 196, 65)),
            );
            let mut meta: Vec<String> = Vec::new();
            if let Some(l) = entry.level {
                meta.push(format!("Level {l}"));
            }
            if let Some(t) = &entry.creature_type {
                meta.push(t.clone());
            }
            if entry.undead {
                meta.push("Undead".into());
            }
            meta.extend(entry.tags.iter().cloned());
            ui.label(egui::RichText::new(meta.join(" · ")).weak());
            if let Some(f) = &entry.family {
                if ui.link(f).clicked() {
                    state.family = f.clone();
                    state.selected = None;
                }
            }
        });

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // ---- Stat tiles -----------------------------------------
                let mut tiles: Vec<(String, String)> = Vec::new();
                if let Some(hp) = entry.max_hp {
                    tiles.push(("HP".into(), hp.to_string()));
                }
                for a in &entry.offense.physical {
                    if let Some(v) = a.value {
                        tiles.push((format!("AS {}", a.name.to_lowercase()), v.display()));
                    }
                }
                for a in &entry.offense.warding {
                    if let Some(v) = a.value {
                        tiles.push(("CS".into(), v.display()));
                    }
                }
                if let Some(d) = &entry.defense {
                    if let Some(asg) = &d.asg {
                        tiles.push(("ASG".into(), asg.clone()));
                    }
                    if let Some(v) = d.melee {
                        tiles.push(("Melee DS".into(), v.display()));
                    }
                    if let Some(v) = d.ranged {
                        tiles.push(("Ranged DS".into(), v.display()));
                    }
                    if let Some(v) = d.bolt {
                        tiles.push(("Bolt DS".into(), v.display()));
                    }
                    if let Some(v) = d.udf {
                        tiles.push(("UDF".into(), v.display()));
                    }
                    for (k, v) in &d.td {
                        tiles.push((format!("TD {k}"), v.display()));
                    }
                }
                if !tiles.is_empty() {
                    let cols = 5usize;
                    egui::Grid::new("bestiary_stats")
                        .num_columns(cols)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            for (i, (label, value)) in tiles.iter().enumerate() {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(label.to_uppercase())
                                            .size(9.0)
                                            .weak(),
                                    );
                                    ui.label(egui::RichText::new(value).strong().size(14.0));
                                });
                                if (i + 1) % cols == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                    ui.separator();
                }

                if let Some(desc) = &entry.description {
                    ui.label(egui::RichText::new(desc).italics().weak());
                    ui.add_space(6.0);
                }

                // ---- Locations ------------------------------------------
                if !entry.spawns.is_empty() || !entry.areas.is_empty() {
                    ui.label(egui::RichText::new("Locations").strong());
                    for spawn in &entry.spawns {
                        ui.horizontal(|ui| {
                            let label = spawn.map.clone().unwrap_or_else(|| "?".into());
                            let rooms: u64 =
                                spawn.uids.iter().map(|(lo, hi)| hi - lo + 1).sum();
                            ui.label(format!("• {label} ({rooms} rooms)"));
                            if let Some((lo, _)) = spawn.uids.first() {
                                if ui.small_button("go2").clicked() {
                                    clicked = Some(GuiLinkClick {
                                        link_data: crate::data::LinkData {
                                            exist_id: crate::data::DIRECT_LINK_SENTINEL
                                                .to_string(),
                                            noun: format!(".go2 u{lo}"),
                                            text: "go2".into(),
                                            coord: None,
                                        },
                                        click_pos: (0, 0),
                                    });
                                }
                            }
                        });
                    }
                    for area in &entry.areas {
                        if !entry
                            .spawns
                            .iter()
                            .any(|s| s.map.as_deref() == Some(area.as_str()))
                        {
                            ui.label(format!("• {area}"));
                        }
                    }
                    ui.add_space(6.0);
                }

                // ---- Offense extras -------------------------------------
                let extras: Vec<String> = entry
                    .offense
                    .spells
                    .iter()
                    .map(|s| format!("Spell: {s}"))
                    .chain(entry.offense.maneuvers.iter().map(|m| format!("Maneuver: {m}")))
                    .chain(entry.offense.specials.iter().map(|s| {
                        match &s.note {
                            Some(n) => format!("Special: {} — {n}", s.name),
                            None => format!("Special: {}", s.name),
                        }
                    }))
                    .collect();
                if !extras.is_empty() {
                    ui.label(egui::RichText::new("Offense").strong());
                    for line in extras {
                        ui.label(format!("• {line}"));
                    }
                    ui.add_space(6.0);
                }
                if let Some(d) = &entry.defense {
                    let extras: Vec<String> = d
                        .immunities
                        .iter()
                        .map(|s| format!("Immune: {s}"))
                        .chain(d.spells.iter().map(|s| format!("Spell: {s}")))
                        .chain(d.abilities.iter().map(|s| format!("Ability: {s}")))
                        .chain(d.specials.iter().map(|s| format!("Special: {s}")))
                        .collect();
                    if !extras.is_empty() {
                        ui.label(egui::RichText::new("Defense").strong());
                        for line in extras {
                            ui.label(format!("• {line}"));
                        }
                        ui.add_space(6.0);
                    }
                }

                // ---- Treasure & notes -----------------------------------
                if !entry.treasure.is_empty() {
                    ui.label(egui::RichText::new("Treasure").strong());
                    let mut items: Vec<String> = Vec::new();
                    if entry.treasure.coins {
                        items.push("coins".into());
                    }
                    if entry.treasure.gems {
                        items.push("gems".into());
                    }
                    if entry.treasure.boxes {
                        items.push("boxes".into());
                    }
                    if entry.treasure.magic {
                        items.push("magic items".into());
                    }
                    if let Some(skin) = &entry.treasure.skin {
                        items.push(skin.clone());
                    }
                    ui.label(items.join(", "));
                    if let Some(other) = &entry.treasure.other {
                        ui.label(egui::RichText::new(other).weak());
                    }
                    ui.add_space(6.0);
                }
                if let Some(notes) = &entry.notes {
                    ui.label(egui::RichText::new("Notes").strong());
                    ui.label(notes);
                }
            });
        clicked
    }
}
