//! Vellum Studio: standalone art-authoring shell. Boots straight into
//! eframe — no network — and rehosts the pool calibrators so frames and
//! creature sprites can be calibrated without launching the game. The
//! Stage is a live creature-field sandbox: fabricated roster + crtrStatus
//! state driven through the REAL field renderer
//! (`render_creature_field_content`), never a reimplementation.

use anyhow::anyhow;
use eframe::egui;

use crate::core::state::{Creature, CreatureFlags, CRTR_STATUS_FLAGS};
use crate::core::AppCore;

use super::app::editors::{CalibrationOutcome, CreatureCalibrationState, FrameCalibrationState};
use super::app::{theme as app_theme, widgets};
use super::persistence::FontRef;
use super::skin;

/// Oldest status lines drop past this.
const STATUS_CAP: usize = 50;

/// Room facts handed over when the client launches Studio — prefill for
/// the new-scene inputs, never applied to anything on its own. Standalone
/// Studio runs with `None` everywhere and stays fully usable.
#[derive(Debug, Clone, Default)]
pub struct StudioRoomContext {
    pub uid: Option<i64>,
    pub title: Option<String>,
    pub location: Option<String>,
}

impl StudioRoomContext {
    fn is_empty(&self) -> bool {
        self.uid.is_none() && self.title.is_none() && self.location.is_none()
    }
}

/// New-scene naming state: an explicit binding choice plus typed inputs,
/// composed into a filename stem through the scene path rules and
/// previewed BEFORE anything is saved. egui-free so tests drive it.
struct NewSceneState {
    binding: crate::config::scenes::SceneBinding,
    /// Room uid input (RoomUid binding), typed or prefilled.
    uid_text: String,
    /// Name input: uid garnish for RoomUid, the matched text otherwise.
    text: String,
    /// Launch-time room facts, kept so switching bindings can re-prefill.
    context: StudioRoomContext,
}

impl NewSceneState {
    /// Prefill the INPUTS from the room context (uid binding when a uid is
    /// known, else title, else location, else default). Only inputs: the
    /// loaded scene and its name are untouched, so launching with room
    /// context never silently overwrites a saved scene.
    fn new(context: StudioRoomContext) -> Self {
        use crate::config::scenes::SceneBinding;
        let binding = if context.uid.is_some() {
            SceneBinding::RoomUid
        } else if context.title.is_some() {
            SceneBinding::RoomTitle
        } else if context.location.is_some() {
            SceneBinding::Location
        } else {
            SceneBinding::Default
        };
        let mut state = Self {
            binding,
            uid_text: String::new(),
            text: String::new(),
            context,
        };
        state.prefill_inputs();
        state
    }

    /// Seed the inputs for the current binding from the room context.
    fn prefill_inputs(&mut self) {
        use crate::config::scenes::SceneBinding;
        self.uid_text = self
            .context
            .uid
            .map(|u| u.to_string())
            .unwrap_or_default();
        self.text = match self.binding {
            SceneBinding::RoomUid | SceneBinding::RoomTitle => {
                self.context.title.clone().unwrap_or_default()
            }
            SceneBinding::Location => self.context.location.clone().unwrap_or_default(),
            SceneBinding::Default => String::new(),
        };
    }

    /// The filename stem these inputs compose to, or why they don't.
    fn preview(&self) -> Result<String, String> {
        crate::config::scenes::compose_stem(self.binding, &self.uid_text, &self.text)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StudioMode {
    Anchorer,
    Stage,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnchorTab {
    Frames,
    Creatures,
}

pub struct StudioApp {
    skin_state: skin::SkinState,
    mode: StudioMode,
    anchor_tab: AnchorTab,
    frame_calibration: Option<FrameCalibrationState>,
    creature_calibration: Option<CreatureCalibrationState>,
    scenery_calibration: Option<super::app::editors::SceneryCalibrationState>,
    /// Auto-open happens once per tab; after the user closes a calibrator
    /// it stays closed until Refresh, so the X isn't fought every frame.
    frames_opened: bool,
    creatures_opened: bool,
    status: Vec<String>,
    styled: bool,
    /// Built lazily on first Stage entry (AppCore::new is FS-only here).
    stage: Option<StageState>,
    /// Room facts from the launching client (new-scene prefill only).
    room_context: StudioRoomContext,
}

/// One castable pool-art entry: a base image's token, humanized.
struct CastEntry {
    /// Display name, '_' -> ' ' (name_token slugs it back to the art).
    display: String,
    /// Last word of the display name.
    noun: String,
}

/// The Stage sandbox: a fabricated AppCore whose room roster and target
/// state feed the production creature-field pipeline.
struct StageState {
    app_core: AppCore,
    cast: Vec<CastEntry>,
    filter: String,
    next_id: u64,
    selected: Option<String>,
    show_grid: bool,
    show_order: bool,
    /// Currently applied rider/mount pair (first flagged rider + mount).
    pair: Option<(String, String)>,
    /// The Stage's scene, held as the Arc the render settings take; edits
    /// go through `Arc::make_mut` (cheap — the render clone is per-frame).
    scene: std::sync::Arc<crate::config::scenes::StageScene>,
    /// Scene name box (save target / last loaded).
    scene_name: String,
    /// Load-list filter box.
    scene_filter: String,
    /// Solver tunables are testing-only UI; hidden unless asked for.
    show_solver_tuning: bool,
    /// Scenery-pool filter box.
    prop_filter: String,
    /// Selected placed prop (index into scene.props); drag-to-place moves it.
    selected_prop: Option<usize>,
    /// Creature exist id being dragged on the stage (grabbed by its card).
    dragging_creature: Option<String>,
    /// Scene Arc the solver obstacles were last built from (`make_mut`
    /// re-points the Arc on every edit, so pointer identity is the dirty
    /// bit — sidecars are only re-read when the scene actually changed).
    obstacle_scene: Option<std::sync::Arc<crate::config::scenes::StageScene>>,
    /// Props-section request to open the scenery calibrator (hosted by
    /// StudioApp, which owns the calibrator states).
    open_scenery_cal: bool,
    /// Status lines raised inside panel closures, drained by stage_ui.
    pending_status: Vec<String>,
    /// New-scene naming inputs (manual binding + name, room prefill).
    new_scene: NewSceneState,
}

/// Fabricated layout window carrying the Stage's grid/order toggles.
const STAGE_WINDOW: &str = "studio-stage";

impl StageState {
    fn new(context: StudioRoomContext) -> anyhow::Result<Self> {
        let config = crate::config::Config::load()?;
        let mut app_core = AppCore::new(config)?;
        // Launch context wins; a fabricated AppCore that somehow carries
        // room facts (it never does standalone) is the fallback.
        let context = if context.is_empty() {
            StudioRoomContext {
                uid: app_core
                    .nav_room_id
                    .as_deref()
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .filter(|&u| u != 0),
                title: app_core.current_room_title(),
                location: app_core.current_room_scope().location,
            }
        } else {
            context
        };
        // In-memory only: the def carries show_grid/show_order for the
        // renderer's per-window lookup. Never saved.
        if let Some(mut def) = crate::core::local_catalog::seed("creaturefield") {
            def.base_mut().name = STAGE_WINDOW.to_string();
            app_core.layout.windows.push(def);
        }
        Ok(Self {
            app_core,
            cast: build_cast(),
            filter: String::new(),
            next_id: 0,
            selected: None,
            show_grid: true,
            show_order: false,
            pair: None,
            scene: std::sync::Arc::new(crate::config::scenes::StageScene::default()),
            scene_name: String::new(),
            scene_filter: String::new(),
            show_solver_tuning: false,
            prop_filter: String::new(),
            selected_prop: None,
            dragging_creature: None,
            obstacle_scene: None,
            open_scenery_cal: false,
            pending_status: Vec::new(),
            new_scene: NewSceneState::new(context),
        })
    }

    fn resync(&mut self) {
        self.app_core.game_state.room_creatures_generation += 1;
        crate::core::creature_cards::sync_field(
            &mut self.app_core.creature_field,
            &mut self.app_core.creature_field_synced_gen,
            &mut self.app_core.creature_field_synced_cal,
            &self.app_core.game_state,
            &[],
        );
        self.refresh_mounts();
    }

    /// First flagged rider pairs with first flagged mount; changes tear
    /// down the old pair (when both halves still stand) before applying.
    fn refresh_mounts(&mut self) {
        let gs = &self.app_core.game_state;
        let rider = gs
            .room_creatures
            .iter()
            .find(|c| c.flags.as_ref().is_some_and(|f| f.rider))
            .map(|c| c.id.clone());
        let mount = gs
            .room_creatures
            .iter()
            .find(|c| c.flags.as_ref().is_some_and(|f| f.mount) && Some(&c.id) != rider.as_ref())
            .map(|c| c.id.clone());
        let desired = rider.zip(mount);
        if desired == self.pair {
            return;
        }
        if let Some((old_rider, _)) = self.pair.take() {
            let still_paired = self
                .app_core
                .creature_field
                .unit_of(&old_rider)
                .is_some_and(|u| u.members.len() > 1);
            if still_paired {
                let (standing, prone) = self
                    .app_core
                    .game_state
                    .room_creatures
                    .iter()
                    .find(|c| c.id == old_rider)
                    .map(crate::core::creature_cards::card_boxes_for)
                    .unwrap_or_default();
                self.app_core
                    .creature_field
                    .dismount(&old_rider, standing, prone);
            }
        }
        if let Some((rider, mount)) = &desired {
            let field = &mut self.app_core.creature_field;
            if field.unit_of(rider).is_some_and(|u| u.members.len() == 1)
                && field.unit_of(mount).is_some_and(|u| u.members.len() == 1)
            {
                field.mount(rider, mount);
                self.pair = desired;
            }
        } else {
            self.pair = None;
        }
    }

    fn spawn(&mut self, display: &str, noun: Option<String>) {
        self.next_id += 1;
        let id = format!("studio-{}", self.next_id);
        self.app_core.game_state.room_creatures.push(Creature {
            name: display.to_string(),
            noun,
            id: id.clone(),
            status: None,
            flags: Some(CreatureFlags {
                hostile: true,
                ..Default::default()
            }),
        });
        self.selected = Some(id);
        self.resync();
    }

    fn remove(&mut self, id: &str) {
        self.app_core
            .game_state
            .room_creatures
            .retain(|c| c.id != id);
        if self.selected.as_deref() == Some(id) {
            self.selected = None;
        }
        if self.app_core.game_state.target_list.current_target == id {
            self.app_core.game_state.target_list.current_target = String::new();
        }
        self.resync();
    }

    fn clear(&mut self) {
        self.app_core.game_state.room_creatures.clear();
        self.selected = None;
        self.app_core.game_state.target_list.current_target = String::new();
        self.resync();
    }

    /// Push the panel toggles into the fabricated layout def the renderer
    /// reads its per-window options from.
    fn apply_view_options(&mut self) {
        if let Some(crate::config::WindowDef::CreatureField { data, .. }) = self
            .app_core
            .layout
            .windows
            .iter_mut()
            .find(|w| w.name() == STAGE_WINDOW)
        {
            data.show_grid = self.show_grid;
            data.show_order = self.show_order;
        }
    }

    /// Art wanted for the current roster — same recipe as the game's
    /// update loop (family from the bestiary, prone + wounds from flags).
    fn wanted_art(&self) -> Vec<super::skin::WantedCreature> {
        self.app_core
            .game_state
            .room_creatures
            .iter()
            .map(|c| {
                let family = c
                    .noun
                    .as_deref()
                    .and_then(crate::core::creature_cards::family_for_noun);
                super::skin::WantedCreature {
                    name: c.name.clone(),
                    noun: c.noun.clone(),
                    family,
                    prone: c.flags.as_ref().is_some_and(|f| f.has_flag("prone")),
                    injuries: c
                        .flags
                        .as_ref()
                        .map(|f| f.injuries.clone())
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    /// The Scene section: backdrop + scenery props, persisted whole-file
    /// to `global/scenes/<name>.toml`.
    fn scene_ui(&mut self, ui: &mut egui::Ui) {
        use crate::config::scenes::{self, SceneProp, StageScene};
        use std::sync::Arc;

        ui.heading("Scene");
        // The scene FILENAME is the binding: leading room uid binds that
        // room, otherwise the stem matches room title -> location ->
        // "default". NEW captures the current room's facts into a stem;
        // there is no separate bindings store to manage.
        ui.label(if self.scene_name.trim().is_empty() {
            "Editing: (unsaved scene)".to_string()
        } else {
            format!("Editing: {}", scenes::display_name(self.scene_name.trim()))
        });
        ui.horizontal_wrapped(|ui| {
            if !self.scene_name.trim().is_empty() && ui.button("Save").clicked() {
                self.save_current_scene();
            }
            if ui.button("Clear stage scene").clicked() {
                self.scene = Arc::new(StageScene::default());
                self.scene_name.clear();
                self.selected_prop = None;
            }
        });
        // New-scene naming: explicit binding + typed name, previewed and
        // validated before it becomes the save target. Room facts (when
        // Studio was launched from the client) only PREFILL the inputs.
        egui::CollapsingHeader::new("New scene…")
            .default_open(false)
            .show(ui, |ui| {
                use crate::config::scenes::SceneBinding;
                let ns = &mut self.new_scene;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bind by");
                    for (binding, label) in [
                        (SceneBinding::RoomUid, "Room uid"),
                        (SceneBinding::RoomTitle, "Room title"),
                        (SceneBinding::Location, "Location"),
                        (SceneBinding::Default, "Default"),
                    ] {
                        if ui
                            .selectable_label(ns.binding == binding, label)
                            .clicked()
                            && ns.binding != binding
                        {
                            ns.binding = binding;
                            ns.prefill_inputs();
                        }
                    }
                });
                match ns.binding {
                    SceneBinding::RoomUid => {
                        ui.horizontal(|ui| {
                            ui.label("Room uid");
                            ui.add(
                                egui::TextEdit::singleline(&mut ns.uid_text)
                                    .hint_text("e.g. 47009")
                                    .desired_width(80.0),
                            );
                            ui.label("garnish");
                            ui.add(
                                egui::TextEdit::singleline(&mut ns.text)
                                    .hint_text("optional room name")
                                    .desired_width(160.0),
                            );
                        });
                    }
                    SceneBinding::RoomTitle | SceneBinding::Location => {
                        ui.horizontal(|ui| {
                            ui.label(if ns.binding == SceneBinding::RoomTitle {
                                "Room title"
                            } else {
                                "Location"
                            });
                            ui.add(
                                egui::TextEdit::singleline(&mut ns.text)
                                    .hint_text("exact in-game text")
                                    .desired_width(220.0),
                            );
                        });
                    }
                    SceneBinding::Default => {
                        ui.weak("The fallback scene every unbound room uses.");
                    }
                }
                match ns.preview() {
                    Ok(stem) => {
                        ui.label(format!("File: global/scenes/{stem}.toml"));
                        let exists = scenes::scene_exists(&stem);
                        let mut adopt = false;
                        let mut load_existing = false;
                        if exists {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                format!(
                                    "Scene '{}' already exists — Save will REPLACE it.",
                                    scenes::display_name(&stem)
                                ),
                            );
                            ui.horizontal(|ui| {
                                adopt = ui
                                    .button("Use name (replace on save)")
                                    .on_hover_text(
                                        "The stage carries over; the next Save overwrites \
                                         the existing scene file",
                                    )
                                    .clicked();
                                load_existing = ui
                                    .button("Load existing instead")
                                    .clicked();
                            });
                        } else {
                            adopt = ui
                                .button("Use name (save-as)")
                                .on_hover_text(
                                    "The stage carries over; Save writes a new scene file",
                                )
                                .clicked();
                        }
                        if adopt {
                            self.scene_name = stem;
                        } else if load_existing {
                            self.load_scene_by_name(&stem);
                        }
                    }
                    Err(err) => {
                        ui.colored_label(ui.visuals().error_fg_color, err);
                    }
                }
            });
        let saved = scenes::list_scenes();
        if !saved.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Load");
                ui.add(
                    egui::TextEdit::singleline(&mut self.scene_filter)
                        .hint_text("filter (number, title, location)")
                        .desired_width(160.0),
                );
            });
            let filter = self.scene_filter.to_ascii_lowercase();
            let mut load_request: Option<String> = None;
            egui::ScrollArea::vertical()
                .id_salt("scene_load_list")
                .max_height(110.0)
                .show(ui, |ui| {
                    for name in &saved {
                        // Menus render `_` as spaces; filter against the
                        // rendered form so users type what they see.
                        let display = scenes::display_name(name);
                        if !filter.is_empty() && !display.to_ascii_lowercase().contains(&filter) {
                            continue;
                        }
                        if ui
                            .selectable_label(self.scene_name == *name, display)
                            .clicked()
                        {
                            load_request = Some(name.clone());
                        }
                    }
                });
            if let Some(name) = load_request {
                self.load_scene_by_name(&name);
            }
        }

        let backgrounds = crate::config::pool::list_category("scenes");
        ui.horizontal(|ui| {
            ui.label("Backdrop");
            let current = self.scene.background.clone();
            let text = current
                .as_deref()
                .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
                .unwrap_or_else(|| "(none)".to_string());
            egui::ComboBox::from_id_salt("scene_bg")
                .selected_text(text)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current.is_none(), "(none)").clicked() {
                        Arc::make_mut(&mut self.scene).background = None;
                    }
                    for image in &backgrounds {
                        let on = current.as_deref() == Some(image.pool_path.as_str());
                        if ui.selectable_label(on, image.display_label()).clicked() {
                            Arc::make_mut(&mut self.scene).background =
                                Some(image.pool_path.clone());
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Color");
            // The picker needs a concrete color; an unset scene edits a
            // neutral slate and only stores once touched (None keeps the
            // default panel fill).
            let mut rgb = self
                .scene
                .background_color
                .as_deref()
                .and_then(parse_hex_rgb)
                .unwrap_or([64, 64, 72]);
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                Arc::make_mut(&mut self.scene).background_color =
                    Some(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]));
            }
            if self.scene.background_color.is_some()
                && ui.small_button("x").on_hover_text("Clear color").clicked()
            {
                Arc::make_mut(&mut self.scene).background_color = None;
            }
        });

        ui.separator();
        ui.heading("Props");
        if ui
            .button("Calibrate scenery…")
            .on_hover_text("Feet anchor, world size, exclusion edges — per image")
            .clicked()
        {
            self.open_scenery_cal = true;
        }
        let pool = crate::config::pool::list_category("scenery");
        if backgrounds.is_empty() && pool.is_empty() {
            ui.weak(
                "Drop PNGs into global/images/scenes (backgrounds, author at 880x470) \
                 or global/images/scenery (props)",
            );
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.prop_filter)
                .hint_text("filter")
                .desired_width(140.0),
        );
        let filter = self.prop_filter.to_ascii_lowercase();
        let mut add_request: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("scene_pool")
            .max_height(110.0)
            .show(ui, |ui| {
                for image in &pool {
                    if !filter.is_empty()
                        && !image.pool_path.to_ascii_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        if ui.small_button("Add").clicked() {
                            add_request = Some(image.pool_path.clone());
                        }
                        ui.label(image.display_label());
                    });
                }
            });
        let (z_near, z_far) = self.app_core.creature_field.depth_range();
        if let Some(image) = add_request {
            // New props spawn at stage centre, mid depth.
            let scene = Arc::make_mut(&mut self.scene);
            scene.props.push(SceneProp {
                image,
                x: crate::core::creature_cards::solver::STAGE_W / 2.0,
                z: (z_near + z_far) / 2.0,
                scale: 1.0,
                shadow: false,
            });
            self.selected_prop = Some(scene.props.len() - 1);
        }
        let placed: Vec<String> = self
            .scene
            .props
            .iter()
            .map(|p| p.image.rsplit('/').next().unwrap_or(&p.image).to_string())
            .collect();
        let mut remove_request: Option<usize> = None;
        for (k, name) in placed.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("x").on_hover_text("Remove").clicked() {
                    remove_request = Some(k);
                }
                if ui
                    .selectable_label(self.selected_prop == Some(k), name)
                    .clicked()
                {
                    self.selected_prop = Some(k);
                }
            });
        }
        if let Some(k) = remove_request {
            Arc::make_mut(&mut self.scene).props.remove(k);
            self.selected_prop = match self.selected_prop {
                Some(s) if s == k => None,
                Some(s) if s > k => Some(s - 1),
                other => other,
            };
        }
        if let Some(k) = self.selected_prop {
            let scene = Arc::make_mut(&mut self.scene);
            if let Some(prop) = scene.props.get_mut(k) {
                ui.horizontal(|ui| {
                    ui.label("x");
                    ui.add(egui::DragValue::new(&mut prop.x).speed(2.0));
                    ui.label("z");
                    ui.add(egui::DragValue::new(&mut prop.z).speed(0.02));
                });
                prop.x = prop
                    .x
                    .clamp(0.0, crate::core::creature_cards::solver::STAGE_W);
                prop.z = prop.z.clamp(z_near, z_far);
                ui.add(egui::Slider::new(&mut prop.scale, 0.1..=5.0).text("scale"));
                ui.checkbox(&mut prop.shadow, "Contact shadow")
                    .on_hover_text("Off by default — most scenery paints its own grounding");
                ui.weak("Drag on the stage to move the selected prop");
            }
        }

    }

    /// Save the on-stage scene under its stem, embedding the live camera/
    /// solver tuning SPARSELY: only what differs from the layer underneath
    /// (the default scene for room/location scenes, the built-in defaults
    /// for the default scene itself). Unset keys inherit live — retune the
    /// default and every scene that didn't pin a value follows.
    fn save_current_scene(&mut self) {
        use crate::core::creature_cards::solver::FieldParams;
        let name = self.scene_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        // Validate before serialization with the same limits loading uses:
        // whatever previews is exactly what a reload will produce.
        {
            let fallback = FieldParams::default();
            let live = &mut self.app_core.creature_field.params;
            if live.sanitize_camera(&fallback) {
                self.pending_status
                    .push("Camera values clamped to valid range before save".to_string());
            }
        }
        let params = self.app_core.creature_field.params.clone();
        let mut base = FieldParams::default();
        if crate::config::scenes::matchable(&name) != "default" {
            if let Some(default_scene) = load_default_scene(Some(&name)) {
                base.apply_camera(&default_scene.camera);
                base.apply_solver(&default_scene.solver);
            }
        }
        {
            let scene = std::sync::Arc::make_mut(&mut self.scene);
            scene.camera = sparse_camera(&params, &base);
            scene.solver = sparse_solver(&params, &base);
        }
        match self.scene.save(&name) {
            Ok(()) => {
                self.pending_status
                    .push(format!("Scene '{name}' saved (camera/solver embedded sparse)"));
                // The game may have this scene bound; re-read it.
                self.app_core.reload_creature_field_files();
            }
            Err(err) => self
                .pending_status
                .push(format!("Scene save failed: {err:#}")),
        }
    }

    /// Load a scene onto the stage, adopting its effective camera/solver
    /// (default-scene layer included, so a sparse room scene shows as it
    /// will in game).
    fn load_scene_by_name(&mut self, name: &str) {
        match crate::config::scenes::StageScene::load(name) {
            Ok(scene) => {
                let default_scene = if crate::config::scenes::matchable(name) == "default" {
                    None
                } else {
                    load_default_scene(Some(name))
                };
                let params = crate::core::creature_cards::resolve_field_params(
                    default_scene.as_ref(),
                    Some(&scene),
                    None,
                    &crate::config::creature_field::FieldOverrides::default(),
                );
                let field = &mut self.app_core.creature_field;
                if field.params != params {
                    field.params = params;
                    field.generation = field.generation.wrapping_add(1);
                }
                self.scene = std::sync::Arc::new(scene);
                self.scene_name = name.to_string();
                self.selected_prop = None;
            }
            Err(err) => self
                .pending_status
                .push(format!("Scene load failed: {err:#}")),
        }
    }

    fn panel_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.scene_ui(ui);
            ui.separator();
            ui.heading("Cast");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("filter")
                        .desired_width(140.0),
                );
                if ui.button("Add generic").clicked() {
                    self.spawn("training dummy", Some("dummy".to_string()));
                }
            });
            let filter = self.filter.to_ascii_lowercase();
            let mut spawn_request: Option<(String, String)> = None;
            egui::ScrollArea::vertical()
                .id_salt("stage_cast")
                .max_height(160.0)
                .show(ui, |ui| {
                    for entry in &self.cast {
                        if !filter.is_empty() && !entry.display.to_ascii_lowercase().contains(&filter)
                        {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("Spawn").clicked() {
                                spawn_request =
                                    Some((entry.display.clone(), entry.noun.clone()));
                            }
                            ui.label(&entry.display);
                        });
                    }
                });
            if let Some((display, noun)) = spawn_request {
                self.spawn(&display, Some(noun));
            }

            ui.separator();
            ui.heading("Roster");
            ui.horizontal(|ui| {
                if ui.button("Clear stage").clicked() {
                    self.clear();
                }
            });
            let roster: Vec<(String, String)> = self
                .app_core
                .game_state
                .room_creatures
                .iter()
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect();
            let mut remove_request: Option<String> = None;
            for (id, name) in &roster {
                ui.horizontal(|ui| {
                    if ui.small_button("x").on_hover_text("Remove").clicked() {
                        remove_request = Some(id.clone());
                    }
                    if ui
                        .selectable_label(self.selected.as_deref() == Some(id), name)
                        .clicked()
                    {
                        self.selected = Some(id.clone());
                    }
                });
            }
            if let Some(id) = remove_request {
                self.remove(&id);
            }

            if let Some(id) = self.selected.clone() {
                ui.separator();
                self.simulator_ui(ui, &id);
            }

            ui.separator();
            ui.heading("Camera");
            // Studio-only tuning; the game never mutates live params.
            // Edits share the loader's bounds (`camera_limits`): the drags
            // are ranged AND the whole camera re-sanitizes after every
            // frame's edits, so a value that would reload differently (or
            // a typed NaN) never reaches the renderer or a saved scene.
            {
                use crate::core::creature_cards::solver::camera_limits::*;
                let params = &mut self.app_core.creature_field.params;
                let before = params.clone();
                egui::Grid::new("stage_camera").num_columns(2).show(ui, |ui| {
                    ui.label("focal");
                    ui.add(
                        egui::DragValue::new(&mut params.focal)
                            .speed(2.0)
                            .range(FOCAL.0..=FOCAL.1),
                    );
                    ui.end_row();
                    ui.label("cam_h");
                    ui.add(
                        egui::DragValue::new(&mut params.cam_h)
                            .speed(0.02)
                            .range(EYE_HEIGHT.0..=EYE_HEIGHT.1),
                    );
                    ui.end_row();
                    ui.label("z0");
                    ui.add(
                        egui::DragValue::new(&mut params.z0)
                            .speed(0.02)
                            .range(NEAR_DEPTH.0..=NEAR_DEPTH.1),
                    );
                    ui.end_row();
                    ui.label("dz");
                    ui.add(
                        egui::DragValue::new(&mut params.dz)
                            .speed(0.02)
                            .range(ROW_DEPTH.0..=ROW_DEPTH.1),
                    );
                    ui.end_row();
                    ui.label("horizon");
                    ui.add(
                        egui::DragValue::new(&mut params.horizon)
                            .speed(1.0)
                            .range(HORIZON.0..=HORIZON.1),
                    );
                    ui.end_row();
                    ui.label("cell_w");
                    ui.add(
                        egui::DragValue::new(&mut params.cell_w)
                            .speed(0.01)
                            .range(CELL_WIDTH.0..=CELL_WIDTH.1),
                    );
                    ui.end_row();
                });
                params.sanitize_camera(&before);
            }
            ui.horizontal_wrapped(|ui| {
                // Reset semantics are explicit: built-in solver defaults,
                // or the values this scene would INHERIT from the default
                // scene layer (only offered for a non-default scene).
                let mut reset_to: Option<
                    crate::core::creature_cards::solver::FieldParams,
                > = None;
                if ui
                    .button("Reset camera (built-in)")
                    .on_hover_text("The solver's built-in default camera")
                    .clicked()
                {
                    reset_to = Some(Default::default());
                }
                let name = self.scene_name.trim();
                if !name.is_empty()
                    && crate::config::scenes::matchable(name) != "default"
                    && ui
                        .button("Reset camera (inherited)")
                        .on_hover_text(
                            "The camera this scene inherits from the default \
                             scene layer — what unpinned keys resolve to",
                        )
                        .clicked()
                {
                    let mut base: crate::core::creature_cards::solver::FieldParams =
                        Default::default();
                    if let Some(default_scene) = load_default_scene(Some(name)) {
                        base.apply_camera(&default_scene.camera);
                    }
                    reset_to = Some(base);
                }
                if let Some(base) = reset_to {
                    let params = &mut self.app_core.creature_field.params;
                    params.focal = base.focal;
                    params.cam_h = base.cam_h;
                    params.z0 = base.z0;
                    params.dz = base.dz;
                    params.horizon = base.horizon;
                    params.cell_w = base.cell_w;
                }
            });
            let params = &mut self.app_core.creature_field.params;

            ui.separator();
            ui.checkbox(&mut self.show_solver_tuning, "Show solver tuning (testing)");
            if !self.show_solver_tuning {
                return;
            }
            ui.heading("Solver");
            ui.weak("Placement tunables. Only future arrivals move — permanence.");
            let solver = &mut params.solver;
            ui.checkbox(&mut solver.zone_on, "Spawn ellipse (off = grid)");
            egui::Grid::new("stage_solver").num_columns(2).show(ui, |ui| {
                ui.label("zone_inset");
                ui.add(egui::DragValue::new(&mut solver.zone_inset).speed(0.005).range(0.0..=0.45));
                ui.end_row();
                ui.label("centre_pull");
                ui.add(egui::DragValue::new(&mut solver.centre_pull).speed(0.01).range(0.0..=5.0));
                ui.end_row();
                ui.label("depth_samples");
                ui.add(egui::DragValue::new(&mut solver.depth_samples).range(1..=41));
                ui.end_row();
                ui.label("depth_jitter");
                ui.add(egui::DragValue::new(&mut solver.depth_jitter).speed(0.01).range(0.0..=1.0));
                ui.end_row();
                ui.label("lateral_jitter");
                ui.add(egui::DragValue::new(&mut solver.lateral_jitter).speed(0.01).range(0.0..=1.0));
                ui.end_row();
                ui.label("depth_spread");
                ui.add(egui::DragValue::new(&mut solver.depth_spread).speed(0.01).range(0.0..=5.0));
                ui.end_row();
                ui.label("row_band_push");
                ui.add(egui::DragValue::new(&mut solver.row_band_push).speed(0.02).range(0.0..=10.0));
                ui.end_row();
                ui.label("row_band_px");
                ui.add(egui::DragValue::new(&mut solver.row_band_px).speed(0.5).range(1.0..=200.0));
                ui.end_row();
                ui.label("occlusion_cap");
                ui.add(egui::DragValue::new(&mut solver.occlusion_cap).speed(0.005).range(0.0..=0.95));
                ui.end_row();
                ui.label("variation");
                ui.add(egui::DragValue::new(&mut solver.variation).speed(0.01).range(0.0..=2.0));
                ui.end_row();
                ui.label("fall_reserve");
                ui.add(egui::DragValue::new(&mut solver.fall_reserve).speed(0.01).range(0.0..=5.0));
                ui.end_row();
                ui.label("relax_steps");
                ui.add(egui::DragValue::new(&mut solver.relax_steps).range(0..=10));
                ui.end_row();
            });
            ui.checkbox(&mut solver.seed_front, "First arrival front + centre");
            ui.checkbox(&mut solver.fall_reserve_hard, "Fall envelope is a hard bound");
            ui.checkbox(&mut solver.shuffle_ties, "Shuffle tie candidates");
            {
                use crate::core::creature_cards::solver::SeparationBasis;
                ui.horizontal(|ui| {
                    ui.label("separation");
                    ui.selectable_value(&mut solver.separation_basis, SeparationBasis::Contact, "contact");
                    ui.selectable_value(&mut solver.separation_basis, SeparationBasis::Card, "card");
                });
            }
            if ui.button("Reset solver").clicked() {
                *solver = Default::default();
            }
        });
    }

    /// State editor for one spawned creature: crtrStatus flags, class
    /// bools, wounds, health, targeting, mounting.
    fn simulator_ui(&mut self, ui: &mut egui::Ui, id: &str) {
        let mut structural = false;
        let is_target = self.app_core.game_state.target_list.current_target == id;
        let mut want_target = is_target;
        {
            let Some(creature) = self
                .app_core
                .game_state
                .room_creatures
                .iter_mut()
                .find(|c| c.id == id)
            else {
                return;
            };
            ui.heading(&creature.name);
            let flags = creature.flags.get_or_insert_with(|| CreatureFlags {
                hostile: true,
                ..Default::default()
            });

            ui.checkbox(&mut want_target, "Target");

            ui.label("Status");
            ui.horizontal_wrapped(|ui| {
                for (_, canonical) in CRTR_STATUS_FLAGS.iter() {
                    let mut on = flags.statuses.iter().any(|s| s == canonical);
                    if ui.checkbox(&mut on, *canonical).changed() {
                        if on {
                            flags.statuses.push((*canonical).to_string());
                        } else {
                            flags.statuses.retain(|s| s != canonical);
                        }
                    }
                }
            });

            ui.label("Class");
            ui.horizontal_wrapped(|ui| {
                // dead flips field membership; boss flips card size: both
                // structural, so the field re-syncs below.
                structural |= ui.checkbox(&mut flags.dead, "dead").changed();
                structural |= ui.checkbox(&mut flags.ascension_boss, "boss").changed();
                structural |= ui.checkbox(&mut flags.mini_boss, "mini boss").changed();
                ui.checkbox(&mut flags.sympathetic, "sympathetic");
                structural |= ui.checkbox(&mut flags.rider, "rider").changed();
                structural |= ui.checkbox(&mut flags.mount, "mount").changed();
            });

            let mut has_health = flags.health.is_some();
            if ui.checkbox(&mut has_health, "Health bar").changed() {
                if has_health {
                    flags.health = Some(100);
                    flags.max_health = Some(100);
                } else {
                    flags.health = None;
                    flags.max_health = None;
                }
            }
            if let Some(health) = flags.health.as_mut() {
                flags.max_health = Some(100);
                let mut hp = *health;
                if ui
                    .add(egui::Slider::new(&mut hp, 0..=100).text("health"))
                    .changed()
                {
                    *health = hp;
                }
            }

            ui.label("Wounds (rank 0-3)");
            egui::Grid::new("stage_wounds").num_columns(2).show(ui, |ui| {
                for part in crate::config::INJURY_AREAS.iter() {
                    let mut rank = flags
                        .injuries
                        .iter()
                        .find(|(p, _)| p == part)
                        .map(|(_, r)| *r)
                        .unwrap_or(0);
                    ui.label(*part);
                    ui.horizontal(|ui| {
                        for r in 0u8..=3 {
                            if ui
                                .selectable_label(rank == r, format!("{r}"))
                                .clicked()
                            {
                                rank = r;
                                flags.injuries.retain(|(p, _)| p != part);
                                if r > 0 {
                                    flags.injuries.push(((*part).to_string(), r));
                                }
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        }
        if want_target != is_target {
            self.app_core.game_state.target_list.current_target =
                if want_target { id.to_string() } else { String::new() };
        }
        if structural {
            self.resync();
        }
    }

    fn field_ui(&mut self, ui: &mut egui::Ui, art: super::skin::SharedCreatureArt) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_grid, "Grid");
            ui.checkbox(&mut self.show_order, "Draw order");
        });
        self.apply_view_options();
        // The renderer allocates the remaining region; remember it for the
        // drag interact below (same origin + size).
        let rect = egui::Rect::from_min_size(ui.next_widget_position(), ui.available_size());
        let settings =
            super::app::WidgetRenderSettings::for_creature_field(art, Some(self.scene.clone()));
        let click = super::app::VellumGuiApp::render_creature_field_content(
            &self.app_core,
            ui,
            STAGE_WINDOW,
            &settings,
        );
        // Drag-to-place: while a prop is selected, dragging the field view
        // moves it on the ground plane (screen → stage → ground, through
        // the renderer's own mapping and the solver's inverse projection).
        if let Some(k) = self.selected_prop {
            let response = ui.interact(
                rect,
                ui.id().with("scene_prop_drag"),
                egui::Sense::drag(),
            );
            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let (sx, sy) =
                        super::app::VellumGuiApp::creature_field_stage_pos(rect, pos);
                    let (x, z) = self.app_core.creature_field.ground_from_screen(sx, sy);
                    let scene = std::sync::Arc::make_mut(&mut self.scene);
                    if let Some(prop) = scene.props.get_mut(k) {
                        prop.x = x;
                        prop.z = z;
                    }
                }
            }
        } else {
            // Creature drag-to-place: grab a card (topmost = nearest under
            // the pointer) and drag its ground point around the floor. The
            // solver inverts the projection; separation is deliberately
            // bypassed — the Stage exists to compose exact arrangements.
            let response = ui.interact(
                rect,
                ui.id().with("stage_creature_drag"),
                egui::Sense::drag(),
            );
            if response.drag_started() {
                self.dragging_creature = response.interact_pointer_pos().and_then(|pos| {
                    let (sx, sy) = super::app::VellumGuiApp::creature_field_stage_pos(rect, pos);
                    let field = &self.app_core.creature_field;
                    let mut hit: Option<(f32, String)> = None;
                    for u in field.units() {
                        let r = field.rect(u);
                        let inside = sx >= r.x0 && sx <= r.x1 && sy >= r.y0 && sy <= r.y1;
                        if inside && hit.as_ref().is_none_or(|(z, _)| r.z < *z) {
                            hit = Some((r.z, u.members[0].clone()));
                        }
                    }
                    hit.map(|(_, id)| id)
                });
            }
            if response.dragged() {
                if let (Some(id), Some(pos)) = (
                    self.dragging_creature.clone(),
                    response.interact_pointer_pos(),
                ) {
                    let (sx, sy) = super::app::VellumGuiApp::creature_field_stage_pos(rect, pos);
                    self.app_core.creature_field.place_at(&id, sx, sy);
                }
            }
            if response.drag_stopped() {
                self.dragging_creature = None;
            }
        }
        // The renderer's click-to-target emits the game command; the Stage
        // applies it locally instead of sending it anywhere.
        if let Some(click) = click {
            if let Some(id) = click.link_data.noun.strip_prefix("target #") {
                self.app_core.game_state.target_list.current_target = id.to_string();
            }
        }
    }
}

/// "#rrggbb" -> srgb components, for seeding the color picker. None for
/// anything malformed (the picker then edits its neutral default).
fn parse_hex_rgb(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if hex.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

/// Distinct castable base tokens from the creature pool: a BASE image is
/// flat `creatures/<stem>.png`, or one whose stem equals its parent folder
/// (noun or variant folder). `default`/`status` folders are reserved;
/// `{token}_`-suffixed extras (prone/wounds) fail the stem==folder test.
fn build_cast() -> Vec<CastEntry> {
    let mut tokens = std::collections::BTreeSet::new();
    for image in crate::config::pool::list_creature_images() {
        let stem = image.stem();
        match image.set.as_deref() {
            None => {}
            Some(set) => {
                if set.split('/').any(|seg| seg == "default" || seg == "status") {
                    continue;
                }
                let parent = set.rsplit('/').next().unwrap_or(set);
                if stem != parent {
                    continue;
                }
            }
        }
        if !stem.is_empty() {
            tokens.insert(stem.to_string());
        }
    }
    // Tier extras beside a flat base ({token}_prone, {token}_chest2, …)
    // are pose/wound art, not castable creatures: drop any stem that
    // extends another stem with an underscore suffix. (Spawning "coyote
    // prone" from coyote_prone.png cast the pose image as its own
    // full-height creature.)
    let extras: Vec<String> = tokens
        .iter()
        .filter(|t| {
            t.rmatch_indices('_')
                .any(|(i, _)| tokens.contains(&t[..i]))
        })
        .cloned()
        .collect();
    for extra in extras {
        tokens.remove(&extra);
    }
    tokens
        .into_iter()
        .map(|token| {
            let display = token.replace('_', " ");
            let noun = display
                .rsplit(' ')
                .next()
                .unwrap_or(&display)
                .to_string();
            CastEntry { display, noun }
        })
        .collect()
}

impl Default for StudioApp {
    fn default() -> Self {
        Self {
            skin_state: skin::SkinState::default(),
            mode: StudioMode::Anchorer,
            anchor_tab: AnchorTab::Frames,
            frame_calibration: None,
            creature_calibration: None,
            scenery_calibration: None,
            frames_opened: false,
            creatures_opened: false,
            status: Vec::new(),
            styled: false,
            stage: None,
            room_context: StudioRoomContext::default(),
        }
    }
}

impl StudioApp {
    /// Font and visuals parity with the game GUI's default theme; fonts
    /// can only be set from inside the egui run loop, hence first-frame.
    fn ensure_style(&mut self, ctx: &egui::Context) {
        if self.styled {
            return;
        }
        ctx.set_fonts(app_theme::build_font_definitions(
            &FontRef::SystemDefault,
            &[],
        ));
        let theme = crate::theme::AppTheme::default();
        let visuals = app_theme::visuals_from_theme(&theme);
        widgets::set_widget_accent(ctx, visuals.selection.bg_fill);
        ctx.set_visuals(visuals);
        self.styled = true;
    }

    fn push_status(&mut self, message: String) {
        self.status.push(message);
        if self.status.len() > STATUS_CAP {
            let excess = self.status.len() - STATUS_CAP;
            self.status.drain(..excess);
        }
    }

    fn drain_outcome(&mut self, outcome: CalibrationOutcome) {
        for message in outcome.messages {
            self.push_status(message);
        }
        if outcome.reload_art {
            self.skin_state.force_reload();
        }
    }

    fn open_frames(&mut self) {
        let (state, outcome) = FrameCalibrationState::open(None);
        self.frame_calibration = state;
        self.frames_opened = true;
        self.drain_outcome(outcome);
    }

    fn open_creatures(&mut self) {
        let (state, outcome) = CreatureCalibrationState::open();
        self.creature_calibration = state;
        self.creatures_opened = true;
        self.drain_outcome(outcome);
    }

    fn anchorer_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.anchor_tab == AnchorTab::Frames, "Frames")
                .clicked()
            {
                self.anchor_tab = AnchorTab::Frames;
            }
            if ui
                .selectable_label(self.anchor_tab == AnchorTab::Creatures, "Creatures")
                .clicked()
            {
                self.anchor_tab = AnchorTab::Creatures;
            }
            if ui
                .button("Refresh")
                .on_hover_text("Re-scan the pool and reopen the calibrator")
                .clicked()
            {
                match self.anchor_tab {
                    AnchorTab::Frames => self.open_frames(),
                    AnchorTab::Creatures => self.open_creatures(),
                }
            }
        });
        ui.add_space(8.0);
        ui.weak(
            "Pick an image in the calibrator window; saves write the sidecar + embed \
             metadata in the PNG",
        );

        match self.anchor_tab {
            AnchorTab::Frames => {
                if !self.frames_opened {
                    self.open_frames();
                }
                if let Some(state) = self.frame_calibration.as_mut() {
                    let outcome = state.ui(ctx);
                    if outcome.closed {
                        self.frame_calibration = None;
                    }
                    self.drain_outcome(outcome);
                }
            }
            AnchorTab::Creatures => {
                if !self.creatures_opened {
                    self.open_creatures();
                }
                if let Some(state) = self.creature_calibration.as_mut() {
                    let outcome = state.ui(ctx);
                    if outcome.closed {
                        self.creature_calibration = None;
                    }
                    self.drain_outcome(outcome);
                }
            }
        }
    }

    fn stage_ui(&mut self, root: &mut egui::Ui, ctx: &egui::Context) {
        if self.stage.is_none() {
            match StageState::new(self.room_context.clone()) {
                Ok(stage) => {
                    self.push_status(format!("Stage ready: {} castable bases", stage.cast.len()));
                    self.stage = Some(stage);
                }
                Err(err) => {
                    self.push_status(format!("Stage init failed: {err:#}"));
                }
            }
        }
        let Some(stage) = self.stage.as_mut() else {
            egui::CentralPanel::default().show(root, |ui| {
                ui.heading("Stage");
                ui.weak("Stage unavailable — see the status bar");
            });
            return;
        };
        // Scene edits re-point the Arc (make_mut); rebuild the solver's
        // AUTHORED exclusion metadata only then. Camera edits need no
        // rebuild at all any more — the solver projects the spans with
        // whatever the current geometry is, per placement decision.
        let scene_changed = stage
            .obstacle_scene
            .as_ref()
            .is_none_or(|prev| !std::sync::Arc::ptr_eq(prev, &stage.scene));
        if scene_changed {
            let obstacles =
                crate::core::creature_cards::scene_obstacles(Some(&stage.scene));
            stage.app_core.creature_field.set_obstacles(obstacles);
            stage.obstacle_scene = Some(stage.scene.clone());
        }
        // Roster sync is generation-gated (cheap when unchanged); art prep
        // is cached, so a settled stage costs a few hash lookups.
        crate::core::creature_cards::sync_field(
            &mut stage.app_core.creature_field,
            &mut stage.app_core.creature_field_synced_gen,
            &mut stage.app_core.creature_field_synced_cal,
            &stage.app_core.game_state,
            &[],
        );
        let wanted = stage.wanted_art();
        if !wanted.is_empty() {
            self.skin_state.prepare_creature_art(ctx, &wanted);
        }
        self.skin_state.prepare_scene_art(ctx, &stage.scene);
        let art = self.skin_state.creature_art();
        egui::Panel::right("stage_panel")
            .default_size(300.0)
            .show(root, |ui| stage.panel_ui(ui));
        egui::CentralPanel::default().show(root, |ui| stage.field_ui(ui, art));
        let pending: Vec<String> = stage.pending_status.drain(..).collect();
        for message in pending {
            self.push_status(message);
        }
        // Scenery calibrator, hosted here so the Stage's Props section can
        // request it.
        if self.stage.as_ref().is_some_and(|s| s.open_scenery_cal) {
            if let Some(stage) = self.stage.as_mut() {
                stage.open_scenery_cal = false;
            }
            if self.scenery_calibration.is_none() {
                let (state, outcome) =
                    super::app::editors::SceneryCalibrationState::open();
                self.scenery_calibration = state;
                self.drain_outcome(outcome);
            }
        }
        if let Some(state) = self.scenery_calibration.as_mut() {
            let outcome = state.ui(ctx);
            if outcome.closed {
                self.scenery_calibration = None;
            }
            if outcome.reload_art {
                // Sidecar changed: exclusion spans and art are both stale.
                self.skin_state.force_reload();
                if let Some(stage) = self.stage.as_mut() {
                    stage.obstacle_scene = None;
                }
            }
            self.drain_outcome(outcome);
        }
    }
}

impl eframe::App for StudioApp {
    // This egui fork's App trait hands the root Ui instead of update(ctx).
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        self.ensure_style(ctx);
        // force_reload only takes effect on the next apply; the early
        // return makes the per-frame call cheap.
        self.skin_state.apply_if_changed(ctx, None);

        egui::Panel::top("studio_mode_bar").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Vellum Studio");
                ui.separator();
                if ui
                    .selectable_label(self.mode == StudioMode::Anchorer, "Anchorer")
                    .clicked()
                {
                    self.mode = StudioMode::Anchorer;
                }
                if ui
                    .selectable_label(self.mode == StudioMode::Stage, "Stage")
                    .clicked()
                {
                    self.mode = StudioMode::Stage;
                }
            });
        });
        egui::Panel::bottom("studio_status_bar").show(root, |ui| {
            match self.status.last() {
                Some(message) => ui.label(message),
                None => ui.weak("Ready"),
            }
            .on_hover_ui(|ui| {
                for message in &self.status {
                    ui.label(message);
                }
            });
        });
        match self.mode {
            StudioMode::Anchorer => {
                egui::CentralPanel::default().show(root, |ui| self.anchorer_ui(ui, ctx));
            }
            StudioMode::Stage => self.stage_ui(root, ctx),
        }
    }
}

/// The "default" scene for layering under a room/location scene, unless
/// `excluding` IS the default (saving/loading the default itself).
fn load_default_scene(excluding: Option<&str>) -> Option<crate::config::scenes::StageScene> {
    let names = crate::config::scenes::list_scenes();
    let default = names
        .iter()
        .find(|name| crate::config::scenes::matchable(name) == "default")?;
    if excluding.is_some_and(|name| name == default.as_str()) {
        return None;
    }
    crate::config::scenes::StageScene::load(default).ok()
}

/// Camera keys that differ from `base` — the sparse embed Save writes so
/// unpinned values inherit from the default scene live.
fn sparse_camera(
    params: &crate::core::creature_cards::solver::FieldParams,
    base: &crate::core::creature_cards::solver::FieldParams,
) -> crate::config::skins::CreatureFieldCamera {
    let diff = |value: f32, base: f32| (value != base).then_some(value);
    crate::config::skins::CreatureFieldCamera {
        focal: diff(params.focal, base.focal),
        eye_height: diff(params.cam_h, base.cam_h),
        near_depth: diff(params.z0, base.z0),
        row_depth: diff(params.dz, base.dz),
        horizon: diff(params.horizon, base.horizon),
        cell_width: diff(params.cell_w, base.cell_w),
    }
}

/// Solver keys that differ from `base`; same sparse discipline.
fn sparse_solver(
    params: &crate::core::creature_cards::solver::FieldParams,
    base: &crate::core::creature_cards::solver::FieldParams,
) -> crate::config::skins::CreatureFieldSolver {
    use crate::core::creature_cards::solver::SeparationBasis;
    let s = &params.solver;
    let b = &base.solver;
    let diff = |value: f32, base: f32| (value != base).then_some(value);
    crate::config::skins::CreatureFieldSolver {
        zone: (s.zone_on != b.zone_on)
            .then(|| if s.zone_on { "ellipse" } else { "grid" }.to_string()),
        zone_inset: diff(s.zone_inset, b.zone_inset),
        centre_pull: diff(s.centre_pull, b.centre_pull),
        depth_samples: (s.depth_samples != b.depth_samples).then_some(s.depth_samples),
        depth_jitter: diff(s.depth_jitter, b.depth_jitter),
        lateral_jitter: diff(s.lateral_jitter, b.lateral_jitter),
        depth_spread: diff(s.depth_spread, b.depth_spread),
        row_band_push: diff(s.row_band_push, b.row_band_push),
        row_band_px: diff(s.row_band_px, b.row_band_px),
        occlusion_cap: diff(s.occlusion_cap, b.occlusion_cap),
        variation: diff(s.variation, b.variation),
        seed_front: (s.seed_front != b.seed_front).then_some(s.seed_front),
        fall_reserve: diff(s.fall_reserve, b.fall_reserve),
        fall_reserve_hard: (s.fall_reserve_hard != b.fall_reserve_hard)
            .then_some(s.fall_reserve_hard),
        separation_basis: (s.separation_basis != b.separation_basis).then(|| {
            match s.separation_basis {
                SeparationBasis::Contact => "contact",
                SeparationBasis::Card => "card",
            }
            .to_string()
        }),
        relax_steps: (s.relax_steps != b.relax_steps).then_some(s.relax_steps),
        shuffle_ties: (s.shuffle_ties != b.shuffle_ties).then_some(s.shuffle_ties),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::scenes::{self, SceneBinding};

    #[test]
    fn new_scene_prefill_fills_inputs_and_never_touches_saved_scenes() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        // A saved scene for the room the client is standing in.
        let saved = scenes::StageScene {
            background: Some("scenes/desert.png".into()),
            ..Default::default()
        };
        saved.save("47009_-_Kitchen_Garden").unwrap();
        let on_disk = std::fs::read_to_string(
            scenes::StageScene::path("47009_-_Kitchen_Garden").unwrap(),
        )
        .unwrap();

        let state = NewSceneState::new(StudioRoomContext {
            uid: Some(47009),
            title: Some("Kitchen Garden".into()),
            location: Some("Castle Anwyn".into()),
        });
        // Prefill picked the uid binding and seeded the inputs.
        assert_eq!(state.binding, SceneBinding::RoomUid);
        assert_eq!(state.uid_text, "47009");
        assert_eq!(state.text, "Kitchen Garden");
        // The preview names the EXISTING scene, flagged as a replace...
        let stem = state.preview().unwrap();
        assert_eq!(stem, "47009_-_Kitchen_Garden");
        assert!(scenes::scene_exists(&stem));
        // ...and nothing was written: prefill never silently overwrites.
        let after = std::fs::read_to_string(
            scenes::StageScene::path("47009_-_Kitchen_Garden").unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk, after);

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn new_scene_standalone_defaults_and_rebinding_reprefills() {
        // Standalone (no context): default binding, empty inputs, valid
        // "default" preview out of the box.
        let mut state = NewSceneState::new(StudioRoomContext::default());
        assert_eq!(state.binding, SceneBinding::Default);
        assert_eq!(state.preview().unwrap(), "default");
        // Typed inputs compose without any game connection.
        state.binding = SceneBinding::RoomTitle;
        state.text = "Barley Field".into();
        assert_eq!(state.preview().unwrap(), "Barley_Field");
        state.binding = SceneBinding::RoomUid;
        state.uid_text = "12".into();
        state.text.clear();
        assert_eq!(state.preview().unwrap(), "12");
        // Unsafe input is rejected at preview, before any save exists.
        state.uid_text = "../evil".into();
        assert!(state.preview().is_err());
        // Context-driven rebinding re-prefills the matching input.
        let mut state = NewSceneState::new(StudioRoomContext {
            uid: None,
            title: Some("Barley Field".into()),
            location: Some("Wehnimer's Landing".into()),
        });
        assert_eq!(state.binding, SceneBinding::RoomTitle);
        assert_eq!(state.text, "Barley Field");
        state.binding = SceneBinding::Location;
        state.prefill_inputs();
        assert_eq!(state.text, "Wehnimer's Landing");
    }

    #[test]
    fn camera_saves_reproduce_valid_effective_params() {
        use crate::core::creature_cards::solver::FieldParams;
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        // Standalone default scene: an out-of-range transient (focal=20)
        // sanitizes to the loader's bound BEFORE serialization, so the
        // save reproduces exactly what previewed.
        let mut params = FieldParams::default();
        params.focal = 20.0;
        params.horizon = 160.0;
        assert!(params.sanitize_camera(&FieldParams::default()));
        assert_eq!(params.focal, 60.0, "previewed value = loader's clamp");
        let mut default_scene = scenes::StageScene::default();
        default_scene.camera = sparse_camera(&params, &FieldParams::default());
        default_scene.save("default").unwrap();
        let loaded_default = scenes::StageScene::load("default").unwrap();
        let resolved = crate::core::creature_cards::resolve_field_params(
            None,
            Some(&loaded_default),
            None,
            &crate::config::creature_field::FieldOverrides::default(),
        );
        assert_eq!(resolved.focal, params.focal);
        assert_eq!(resolved.horizon, 160.0);

        // Room scene over the default-scene layer: nonfinite transient
        // falls back to the inherited value; sparse save pins only what
        // differs, and the layered reload reproduces the effective params.
        let mut base = FieldParams::default();
        base.apply_camera(&loaded_default.camera);
        let mut room_params = base.clone();
        room_params.cam_h = f32::NAN; // invalid transient edit
        assert!(room_params.sanitize_camera(&base));
        assert_eq!(room_params.cam_h, base.cam_h, "NaN never reaches params");
        room_params.focal = 300.0;
        let mut room = scenes::StageScene::default();
        room.camera = sparse_camera(&room_params, &base);
        assert!(room.camera.horizon.is_none(), "inherited keys stay sparse");
        room.save("Barley_Field").unwrap();
        let loaded_room = scenes::StageScene::load("Barley_Field").unwrap();
        let resolved = crate::core::creature_cards::resolve_field_params(
            Some(&loaded_default),
            Some(&loaded_room),
            None,
            &crate::config::creature_field::FieldOverrides::default(),
        );
        assert_eq!(resolved.focal, 300.0);
        assert_eq!(resolved.horizon, 160.0, "unpinned key inherits live");
        assert_eq!(resolved.cam_h, base.cam_h);

        std::env::remove_var("VELLUM_FE_DIR");
    }
}

/// Boot the Studio window. `context` carries the launching client's room
/// facts (new-scene prefill); standalone launches pass `None`.
pub fn run_studio(context: Option<StudioRoomContext>) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vellum Studio")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    let context = context.unwrap_or_default();
    eframe::run_native(
        "Vellum Studio",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(StudioApp {
                room_context: context,
                ..Default::default()
            }))
        }),
    )
    .map_err(|err| anyhow!("Failed to run Vellum Studio: {}", err))
}
