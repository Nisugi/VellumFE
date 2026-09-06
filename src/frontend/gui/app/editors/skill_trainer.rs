//! Native skill trainer ("Skill Goals") panel: an egui window over the
//! parsed play.net skill manager page (`data/skill_trainer.rs`). Core owns
//! all state and the point engine — `ui_state.skill_trainer` — so this
//! panel is pure presentation plus deferred calls into the AppCore API
//! (`skill_trainer_step` / `_apply` / `_reload_command` / profiles).
//!
//! Opened by `.goals` (core sets `.open`) or the top-bar "Skill Goals"
//! button; closing the window clears `.open` but keeps the cached page so
//! reopening is instant.

use super::super::VellumGuiApp;
use crate::data::skill_trainer::TrainerStatus;
use eframe::egui;

/// Phy pool numbers (greenish, matches the website's physical column).
const PHY_COLOR: egui::Color32 = egui::Color32::from_rgb(0x7f, 0xc9, 0x7f);
/// Mnt pool numbers (bluish).
const MNT_COLOR: egui::Color32 = egui::Color32::from_rgb(0x7f, 0xa9, 0xe0);

/// GUI-local chrome for the trainer window. All goal data lives in core;
/// this is just the profile form, the step selector, and a profiles cache
/// so we don't hit `skill_goal_profiles.toml` on disk every frame.
pub(in super::super) struct SkillTrainerPanelState {
    /// Default ranks per +/- click (1/10/100); Ctrl/Shift still override.
    default_step: u32,
    /// "Save as" name field in the Profiles menu.
    profile_name: String,
    /// Cached `skill_trainer_profiles()` (file read); None = refetch.
    profiles: Option<Vec<String>>,
    /// Data revision the cache was built against.
    profiles_revision: u64,
}

impl Default for SkillTrainerPanelState {
    fn default() -> Self {
        Self {
            default_step: 1,
            profile_name: String::new(),
            profiles: None,
            profiles_revision: 0,
        }
    }
}

/// What the window closure asked for; applied after it returns because the
/// handlers need `&mut self.app_core` (the closure borrows the data clone).
#[derive(Default)]
struct Actions {
    steps: Vec<(u32, u32, bool)>, // (id, n, raise)
    apply: bool,
    reload: bool,
    /// Open the play.net web skill manager in the system browser.
    open_web: bool,
    save_profile: Option<String>,
    load_profile: Option<String>,
    delete_profile: Option<String>,
}

impl VellumGuiApp {
    /// Top-bar entry point: surface the cached panel, or send GOALS for a
    /// fresh page when nothing is loaded yet.
    pub(in super::super) fn open_skill_trainer_panel(&mut self) {
        if self.app_core.ui_state.skill_trainer.open {
            self.raise_editor(egui::Id::new("gui_skill_trainer_panel"));
            return;
        }
        if self.app_core.ui_state.skill_trainer.data.is_some() {
            self.app_core.ui_state.skill_trainer.open = true;
        } else {
            let cmd = self.app_core.skill_trainer_reload_command();
            self.dispatch_command(cmd);
        }
    }

    pub(super) fn render_skill_trainer_panel(&mut self, ctx: &egui::Context) {
        if !self.app_core.ui_state.skill_trainer.open {
            return;
        }

        // Refresh the profiles cache when the page data was replaced (a
        // reload can switch characters, which switches the profile store).
        let revision = self.app_core.ui_state.skill_trainer.revision;
        if self.skill_trainer_panel.profiles.is_none()
            || self.skill_trainer_panel.profiles_revision != revision
        {
            self.skill_trainer_panel.profiles = Some(self.app_core.skill_trainer_profiles());
            self.skill_trainer_panel.profiles_revision = revision;
        }

        // Clone what the closure reads so it doesn't hold a borrow of
        // app_core (the rows are small; this is per-frame immediate mode).
        let status = self.app_core.ui_state.skill_trainer.status.clone();
        let data = self.app_core.ui_state.skill_trainer.data.clone();
        let state = &mut self.skill_trainer_panel;
        let mut actions = Actions::default();
        let mut open = true;

        egui::Window::new("Skill Goals")
            .id(egui::Id::new("gui_skill_trainer_panel"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .default_height(560.0)
            .min_width(360.0)
            .min_height(220.0)
            .show(ctx, |ui| {
                // Without a fill child the window auto-shrinks to its
                // status strip (and offers no resize edge to grab) while
                // loading or errored — claim the space in every state.
                ui.set_min_size(egui::vec2(ui.available_width(), 180.0));
                // Status strip: lifecycle first, so a load/submit/error is
                // never buried under a stale table.
                match &status {
                    TrainerStatus::Loading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak("Fetching the skill manager page…");
                        });
                    }
                    TrainerStatus::Applying => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak("Submitting skill goals…");
                        });
                    }
                    TrainerStatus::Error(msg) => {
                        ui.horizontal(|ui| {
                            ui.colored_label(ui.visuals().error_fg_color, msg);
                            if ui.button("Retry").clicked() {
                                actions.reload = true;
                            }
                        });
                    }
                    TrainerStatus::Idle => {}
                }

                let Some(goals) = &data else {
                    if status == TrainerStatus::Idle {
                        ui.weak("No skill data — Reload to fetch the page from play.net.");
                        if ui.button("Reload").clicked() {
                            actions.reload = true;
                        }
                    }
                    return;
                };
                let dirty = goals.dirty();
                let idle = status == TrainerStatus::Idle;

                // Header: identity, point pools, controls.
                ui.horizontal(|ui| {
                    ui.strong(format!(
                        "{} · Level {} {}",
                        goals.char_name, goals.level, goals.prof_name
                    ));
                    if !goals.race_name.is_empty() {
                        ui.weak(&goals.race_name);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Points:");
                    let phy_color = if goals.phy_left == 0 && dirty {
                        ui.visuals().error_fg_color
                    } else {
                        PHY_COLOR
                    };
                    let mnt_color = if goals.mnt_left == 0 && dirty {
                        ui.visuals().error_fg_color
                    } else {
                        MNT_COLOR
                    };
                    ui.colored_label(phy_color, goals.phy_left.to_string());
                    ui.label("Phy");
                    ui.colored_label(mnt_color, goals.mnt_left.to_string());
                    ui.label("Mnt");
                    if goals.phy_conv > 0 || goals.mnt_conv > 0 {
                        ui.weak(format!(
                            "({} P>M / {} M>P)",
                            goals.phy_conv, goals.mnt_conv
                        ));
                    }
                });
                ui.horizontal(|ui| {
                    // Default click step; Ctrl (×10) / Shift (×100) still
                    // override per click.
                    ui.label("Step:");
                    for step in [1u32, 10, 100] {
                        if ui
                            .selectable_label(state.default_step == step, format!("±{step}"))
                            .clicked()
                        {
                            state.default_step = step;
                        }
                    }
                    ui.separator();

                    ui.menu_button("Profiles", |ui| {
                        ui.set_min_width(180.0);
                        let profiles = state.profiles.clone().unwrap_or_default();
                        if profiles.is_empty() {
                            ui.weak("No saved profiles.");
                        }
                        for name in &profiles {
                            ui.horizontal(|ui| {
                                if ui.button(name).on_hover_text("Load into editor").clicked()
                                {
                                    actions.load_profile = Some(name.clone());
                                    ui.close();
                                }
                                if ui
                                    .small_button("✕")
                                    .on_hover_text("Delete this profile")
                                    .clicked()
                                {
                                    actions.delete_profile = Some(name.clone());
                                }
                            });
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.profile_name)
                                    .hint_text("profile name")
                                    .desired_width(120.0),
                            );
                            let name = state.profile_name.trim().to_string();
                            if ui
                                .add_enabled(!name.is_empty(), egui::Button::new("Save"))
                                .on_hover_text("Save current goals under this name")
                                .clicked()
                            {
                                actions.save_profile = Some(name);
                                ui.close();
                            }
                        });
                    });

                    if ui
                        .add_enabled(dirty && idle, egui::Button::new("Apply"))
                        .on_hover_text("Submit these goals to play.net")
                        .clicked()
                    {
                        actions.apply = true;
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new("Reload"))
                        .on_hover_text("Re-fetch the page (sends GOALS to the game)")
                        .clicked()
                    {
                        actions.reload = true;
                    }
                    if ui
                        .button("Open in browser")
                        .on_hover_text("Open the play.net web skill manager instead (sends GOALS)")
                        .clicked()
                    {
                        actions.open_web = true;
                    }
                });
                ui.separator();

                // Body: the skill table, grouped under section headers.
                let modifiers = ui.input(|i| i.modifiers);
                let click_step = if modifiers.shift {
                    100
                } else if modifiers.command {
                    10
                } else {
                    state.default_step
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut section = "";
                        egui::Grid::new("skill_goals_grid")
                            .num_columns(7)
                            .striped(true)
                            .show(ui, |ui| {
                                for row in &goals.rows {
                                    if row.section != section {
                                        section = &row.section;
                                        // Section header spans the row.
                                        ui.strong(section);
                                        for _ in 0..6 {
                                            ui.label("");
                                        }
                                        ui.end_row();
                                    }
                                    let start = goals.start_ranks_of(row.id);
                                    let goal = goals.goal_ranks(row.id);
                                    let max = goals.max_ranks_of(row.id);
                                    let (pc, mc) = goals.cost_to_raise(row.id);

                                    ui.label(&row.name);
                                    ui.weak(format!("{pc}/{mc}"))
                                        .on_hover_text("Phy/Mnt cost for the next rank");
                                    ui.weak(start.to_string())
                                        .on_hover_text("Current (committed) ranks");
                                    if ui
                                        .add_enabled(idle, egui::Button::new("−").small())
                                        .clicked()
                                    {
                                        actions.steps.push((row.id, click_step, false));
                                    }
                                    if goal != start {
                                        ui.colored_label(
                                            ui.visuals().warn_fg_color,
                                            goal.to_string(),
                                        );
                                    } else {
                                        ui.label(goal.to_string());
                                    }
                                    if ui
                                        .add_enabled(idle, egui::Button::new("+").small())
                                        .clicked()
                                    {
                                        actions.steps.push((row.id, click_step, true));
                                    }
                                    ui.weak(format!("max {max}"));
                                    ui.end_row();
                                }
                            });
                    });
            });

        // Apply deferred actions after the closure (they need &mut app_core).
        for (id, n, raise) in actions.steps {
            self.app_core.skill_trainer_step(id, n, raise);
        }
        if actions.apply {
            self.app_core.skill_trainer_apply();
        }
        if actions.open_web {
            self.dispatch_command("goals web".to_string());
        }
        if let Some(name) = actions.save_profile {
            self.app_core.skill_trainer_save_profile(&name);
            self.skill_trainer_panel.profiles = None; // refetch next frame
        }
        if let Some(name) = actions.load_profile {
            self.app_core.skill_trainer_load_profile(&name);
        }
        if let Some(name) = actions.delete_profile {
            self.app_core.skill_trainer_delete_profile(&name);
            self.skill_trainer_panel.profiles = None;
        }
        if actions.reload {
            // "goals" must reach the game exactly like a typed command.
            let cmd = self.app_core.skill_trainer_reload_command();
            self.dispatch_command(cmd);
        }

        if !open {
            // Close hides the window; the fetched page stays cached so
            // `.goals` reopens instantly.
            self.app_core.ui_state.skill_trainer.open = false;
        }
    }
}
