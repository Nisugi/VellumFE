//! Stage-scene selection and creature-field camera/solver resolution.
//!
//! The scene FILENAME is the binding (owner decision, no bindings file):
//! a stem leading with a room uid binds that room (rest of the stem is
//! human garnish); otherwise the stem matches the room title, then the
//! mapdb location, then the literal "default" — see
//! `config::scenes::resolve_scene`. Params layer defaults → default scene
//! → matched scene → blanket override → per-scene override via
//! `creature_cards::resolve_field_params`.

use super::AppCore;
use crate::config::scenes::{self, StageScene};
use std::sync::Arc;

impl AppCore {
    /// The room title scenes match against: `game_state.room_name` when
    /// the wire set it, else the `<streamWindow>` subtitle stripped of its
    /// leading " - " and brackets (real sessions rarely set room_name —
    /// same fallback the room widget and remote snapshot use). Never a
    /// Lich-decorated string: both sources are read straight off the wire.
    /// Public: Studio's NEW buttons capture the same title scenes match.
    pub fn current_room_title(&self) -> Option<String> {
        if let Some(name) = self
            .game_state
            .room_name
            .as_ref()
            .filter(|name| !name.trim().is_empty())
        {
            return Some(name.clone());
        }
        self.room_subtitle
            .as_ref()
            .map(|subtitle| {
                subtitle
                    .trim()
                    .trim_start_matches('-')
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string()
            })
            .filter(|title| !title.is_empty())
    }

    /// Pick the creature field's stage scene for the current room and
    /// re-resolve the field's camera/solver params when the pick or the
    /// overrides changed. Steady state is a tuple compare plus an
    /// overrides compare — no file IO, no directory listing, no param
    /// rebuild, no clamp-warning spam.
    pub fn tick_stage_scene(&mut self) {
        let uid = self
            .nav_room_id
            .as_deref()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|&u| u != 0);
        let title = self.current_room_title();
        let location = self.current_room_scope().location;
        let inputs = (uid, title, location);
        if self.scene_pick_inputs.as_ref() != Some(&inputs) {
            let names = scenes::list_scenes();
            let picked = scenes::resolve_scene(
                &names,
                inputs.0,
                inputs.1.as_deref(),
                inputs.2.as_deref(),
            )
            .map(str::to_owned);
            if picked != self.stage_scene_name {
                self.stage_scene = picked.as_deref().and_then(load_scene);
                self.stage_scene_name = picked;
                // A new scene changes the camera; force a re-resolve.
                self.field_params_inputs = None;
            }
            // The default scene is the global camera home; every resolve
            // needs it, so cache it alongside the pick.
            let default_name = names
                .iter()
                .find(|name| scenes::matchable(name) == "default")
                .cloned();
            if default_name != self.default_scene_name {
                self.default_stage_scene = default_name.as_deref().and_then(load_scene);
                self.default_scene_name = default_name;
                self.field_params_inputs = None;
            }
            self.scene_pick_inputs = Some(inputs);
        }
        let unchanged = self.field_params_inputs.as_ref().is_some_and(|(name, ov)| {
            *name == self.stage_scene_name && *ov == self.field_overrides
        });
        if unchanged {
            return;
        }
        // Don't double-apply when the matched scene IS the default (a
        // no-match pick falls back to the default as the scene itself).
        let default_layer = (self.stage_scene_name != self.default_scene_name)
            .then_some(())
            .and(self.default_stage_scene.as_deref());
        let params = crate::core::creature_cards::resolve_field_params(
            default_layer,
            self.stage_scene.as_deref(),
            self.stage_scene_name.as_deref(),
            &self.field_overrides,
        );
        if self.creature_field.params != params {
            self.creature_field.params = params;
            // Renderers cache quads against the generation; a camera change
            // repositions every drawn card.
            self.creature_field.generation = self.creature_field.generation.wrapping_add(1);
        }
        // Prop exclusion metadata is authored (world units); the solver
        // projects it per placement decision, so this rebuild only needs
        // to track the scene/sidecar data itself (scene changes always
        // reset field_params_inputs and land here).
        let obstacles =
            crate::core::creature_cards::scene_obstacles(self.stage_scene.as_deref());
        self.creature_field.set_obstacles(obstacles);
        self.field_params_inputs =
            Some((self.stage_scene_name.clone(), self.field_overrides.clone()));
    }

    /// Re-read the creature-field files from disk (scenes may have been
    /// re-saved under the same name — Studio runs as a separate process)
    /// and force a full re-pick + re-resolve on the next tick. In-memory
    /// `field_overrides` edits don't need it — the tick compare catches
    /// those.
    pub fn reload_creature_field_files(&mut self) {
        self.field_overrides = crate::config::creature_field::FieldOverrides::load();
        // Sidecar calibration may have changed with the files; drop the
        // shared store so geometry re-seeds (placed units recalibrate in
        // place on the next sync — nobody moves).
        crate::core::creature_cards::geometry::calibrations()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate();
        self.stage_scene = None;
        self.stage_scene_name = None;
        self.default_stage_scene = None;
        self.default_scene_name = None;
        self.scene_pick_inputs = None;
        self.field_params_inputs = None;
    }
}

fn load_scene(name: &str) -> Option<Arc<StageScene>> {
    match StageScene::load(name) {
        Ok(scene) => Some(Arc::new(scene)),
        Err(err) => {
            tracing::warn!("stage scene '{name}' not loadable: {err:#}");
            None
        }
    }
}
