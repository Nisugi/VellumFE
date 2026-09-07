//! Creature-field override editor (`.creaturefield` / `.fieldcamera`):
//! live camera/solver overrides layered over the active scene, stored in
//! `global/creature_field.toml`. Scope toggle picks between a per-scene
//! override (wins) and the blanket all-scenes override; every knob applies
//! to the live field immediately (core re-resolves on the next tick), Save
//! persists, Remove clears the scope back to the layer underneath.

use super::super::VellumGuiApp;
use eframe::egui;

/// Which override the knobs edit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The override for the scene active when the editor targets it.
    Scene,
    /// The blanket override (every scene, and sceneless play).
    AllScenes,
}

pub(crate) struct CreatureFieldEditorState {
    scope: Scope,
}

impl CreatureFieldEditorState {
    fn new() -> Self {
        Self {
            scope: Scope::AllScenes,
        }
    }
}

/// One optional camera/solver knob: checkbox arms the override (seeded
/// from the effective live value), drag edits it while armed.
fn opt_f32_row(
    ui: &mut egui::Ui,
    label: &str,
    slot: &mut Option<f32>,
    effective: f32,
    speed: f64,
) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        let mut on = slot.is_some();
        if ui.checkbox(&mut on, "").on_hover_text("Override this value").changed() {
            *slot = on.then_some(effective);
            changed = true;
        }
        match slot {
            Some(value) => {
                changed |= ui.add(egui::DragValue::new(value).speed(speed)).changed();
            }
            None => {
                ui.weak(format!("{effective:.2}"));
            }
        }
    });
    ui.end_row();
    changed
}

impl VellumGuiApp {
    pub(in super::super) fn open_creature_field_editor(&mut self) {
        if self.creature_field_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_creature_field_editor"));
            return;
        }
        // Studio runs as a separate process; opening the editor is the
        // natural "I just edited scenes over there" moment, so re-read.
        self.app_core.reload_creature_field_files();
        self.creature_field_editor = Some(CreatureFieldEditorState::new());
    }

    /// Launch Vellum Studio (the scene editor) as its own process — it has
    /// its own eframe event loop, so it cannot run inside this window.
    /// Scene edits land on disk; "Reload scene files" (or reopening this
    /// editor) picks them up here.
    pub(in super::super) fn launch_studio(&mut self) {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Cannot locate vellum-fe to launch Studio: {err}"));
                return;
            }
        };
        let mut cmd = std::process::Command::new(exe);
        // Dead std handles from a consoleless parent break Stdio::inherit
        // (os error 50); Studio logs to file, so null them all.
        cmd.arg("studio");
        // Snapshot the current room so Studio can prefill new-scene naming
        // (prefill only — Studio never saves anything from these).
        if let Some(uid) = self
            .app_core
            .nav_room_id
            .as_deref()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|&u| u > 0)
        {
            cmd.arg("--room-uid").arg(uid.to_string());
        }
        if let Some(title) = self.app_core.current_room_title() {
            cmd.arg("--room-title").arg(title);
        }
        if let Some(location) = self.app_core.current_room_scope().location {
            cmd.arg("--room-location").arg(location);
        }
        cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        match cmd.spawn() {
            Ok(_) => self.app_core.add_system_message(
                "Vellum Studio launched. After saving scenes there, use \
                 .creaturefield ▸ Reload scene files to apply them here.",
            ),
            Err(err) => self
                .app_core
                .add_system_message(&format!("Cannot launch Vellum Studio: {err}")),
        }
    }

    pub(in super::super) fn render_creature_field_editor(&mut self, ctx: &egui::Context) {
        let Some(state) = self.creature_field_editor.as_mut() else {
            return;
        };
        let mut open = true;
        let mut save_request = false;
        let mut remove_request = false;
        let mut reload_request = false;
        let mut messages: Vec<String> = Vec::new();

        let scene_name = self.app_core.stage_scene_name.clone();
        // Per-scene scope needs a scene; fall back rather than editing a
        // phantom entry.
        if scene_name.is_none() && state.scope == Scope::Scene {
            state.scope = Scope::AllScenes;
        }
        let effective = self.app_core.creature_field.params.clone();

        egui::Window::new("Creature Field")
            .id(egui::Id::new("gui_creature_field_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(340.0)
            .resizable(true)
            .show(ctx, |ui| {
                match &scene_name {
                    Some(name) => ui.label(format!(
                        "Active scene: {}",
                        crate::config::scenes::display_name(name)
                    )),
                    None => ui.label("Active scene: (none)"),
                };
                ui.weak(
                    "Overrides layer over the scene's own camera/solver: \
                     this-scene wins over all-scenes wins over the scene. \
                     Changes apply live; Save persists them.",
                );
                ui.horizontal(|ui| {
                    ui.label("Apply to:");
                    if let Some(name) = &scene_name {
                        ui.selectable_value(
                            &mut state.scope,
                            Scope::Scene,
                            format!("This scene ({})", crate::config::scenes::display_name(name)),
                        );
                    } else {
                        ui.add_enabled(false, egui::Button::new("This scene (none active)"));
                    }
                    ui.selectable_value(&mut state.scope, Scope::AllScenes, "All scenes");
                });
                ui.separator();

                let overrides = &mut self.app_core.field_overrides;
                let entry = match state.scope {
                    Scope::AllScenes => &mut overrides.blanket,
                    Scope::Scene => overrides
                        .scenes
                        .entry(scene_name.clone().unwrap_or_default())
                        .or_default(),
                };

                ui.checkbox(&mut entry.enabled, "Enabled")
                    .on_hover_text("Off keeps the values but stops applying them.");

                ui.label("Camera");
                egui::Grid::new("cf_override_camera").num_columns(2).show(ui, |ui| {
                    let cam = &mut entry.camera;
                    opt_f32_row(ui, "focal", &mut cam.focal, effective.focal, 2.0);
                    opt_f32_row(ui, "eye_height", &mut cam.eye_height, effective.cam_h, 0.02);
                    opt_f32_row(ui, "near_depth", &mut cam.near_depth, effective.z0, 0.02);
                    opt_f32_row(ui, "row_depth", &mut cam.row_depth, effective.dz, 0.02);
                    opt_f32_row(ui, "horizon", &mut cam.horizon, effective.horizon, 1.0);
                    opt_f32_row(ui, "cell_width", &mut cam.cell_width, effective.cell_w, 0.01);
                });

                ui.collapsing("Solver", |ui| {
                    ui.weak("Placement tunables — only future arrivals move.");
                    let sol = &mut entry.solver;
                    let live = &effective.solver;
                    ui.horizontal(|ui| {
                        ui.label("zone");
                        let current = sol.zone.clone();
                        if ui
                            .selectable_label(current.is_none(), "(scene)")
                            .clicked()
                        {
                            sol.zone = None;
                        }
                        if ui
                            .selectable_label(current.as_deref() == Some("ellipse"), "ellipse")
                            .clicked()
                        {
                            sol.zone = Some("ellipse".to_string());
                        }
                        if ui
                            .selectable_label(current.as_deref() == Some("grid"), "grid")
                            .clicked()
                        {
                            sol.zone = Some("grid".to_string());
                        }
                    });
                    egui::Grid::new("cf_override_solver").num_columns(2).show(ui, |ui| {
                        opt_f32_row(ui, "zone_inset", &mut sol.zone_inset, live.zone_inset, 0.005);
                        opt_f32_row(ui, "centre_pull", &mut sol.centre_pull, live.centre_pull, 0.01);
                        opt_f32_row(ui, "depth_jitter", &mut sol.depth_jitter, live.depth_jitter, 0.01);
                        opt_f32_row(
                            ui,
                            "lateral_jitter",
                            &mut sol.lateral_jitter,
                            live.lateral_jitter,
                            0.01,
                        );
                        opt_f32_row(ui, "depth_spread", &mut sol.depth_spread, live.depth_spread, 0.01);
                        opt_f32_row(
                            ui,
                            "row_band_push",
                            &mut sol.row_band_push,
                            live.row_band_push,
                            0.02,
                        );
                        opt_f32_row(ui, "row_band_px", &mut sol.row_band_px, live.row_band_px, 0.5);
                        opt_f32_row(
                            ui,
                            "occlusion_cap",
                            &mut sol.occlusion_cap,
                            live.occlusion_cap,
                            0.005,
                        );
                        opt_f32_row(ui, "variation", &mut sol.variation, live.variation, 0.01);
                        opt_f32_row(ui, "fall_reserve", &mut sol.fall_reserve, live.fall_reserve, 0.01);
                    });
                    ui.weak(
                        "depth_samples / relax_steps / the boolean toggles are scene- \
                         and Studio-level tuning; override them by editing the scene.",
                    );
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_request = true;
                    }
                    if ui
                        .button("Remove override")
                        .on_hover_text("Delete this scope's override entirely.")
                        .clicked()
                    {
                        remove_request = true;
                    }
                    if ui
                        .button("Reload scene files")
                        .on_hover_text(
                            "Re-read scenes, bindings, and overrides from disk — \
                             use after saving in Vellum Studio.",
                        )
                        .clicked()
                    {
                        reload_request = true;
                    }
                });

                // Saved per-scene overrides, so stale ones are visible.
                let stale: Vec<String> = self
                    .app_core
                    .field_overrides
                    .scenes
                    .keys()
                    .cloned()
                    .collect();
                if !stale.is_empty() {
                    ui.separator();
                    ui.label("Per-scene overrides:");
                    let mut drop: Option<String> = None;
                    for name in stale {
                        ui.horizontal(|ui| {
                            if ui.small_button("x").on_hover_text("Remove").clicked() {
                                drop = Some(name.clone());
                            }
                            ui.label(crate::config::scenes::display_name(&name));
                        });
                    }
                    if let Some(name) = drop {
                        self.app_core.field_overrides.scenes.remove(&name);
                        save_request = true;
                    }
                }
            });

        if remove_request {
            match state.scope {
                Scope::AllScenes => {
                    self.app_core.field_overrides.blanket = Default::default();
                }
                Scope::Scene => {
                    if let Some(name) = &scene_name {
                        self.app_core.field_overrides.scenes.remove(name);
                    }
                }
            }
            save_request = true;
        }
        if reload_request {
            self.app_core.reload_creature_field_files();
            messages.push("Scene files reloaded.".to_string());
        }
        if save_request {
            match self.app_core.field_overrides.save() {
                Ok(()) => messages.push("Creature-field overrides saved.".to_string()),
                Err(err) => messages.push(format!("Creature-field overrides not saved: {err:#}")),
            }
        }
        for message in messages {
            self.app_core.add_system_message(&message);
        }
        if !open {
            self.creature_field_editor = None;
        }
    }
}
