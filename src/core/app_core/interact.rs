//! Interact mode: pointer-free focus cycling over room entities.
//!
//! A frontend puts the app in `InputMode::Interact` (keybind action
//! `interact_mode`), then routes arrow/enter/escape input here. Up/down
//! cycle entities within a category, left/right switch categories,
//! activate opens the server context menu (or walks an exit). Focus is
//! remembered by stable key (exist id / exit direction) so room updates
//! rewriting the entity lists don't steal it.

use super::AppCore;
use crate::data::ui_state::{InputMode, InteractCategory, InteractState};

/// What activating the focused entity should do.
#[derive(Clone, Debug, PartialEq)]
pub enum InteractAction {
    /// Server context-menu round-trip (`_menu #<exist> <counter>`).
    Menu { exist_id: String, noun: String },
    /// Direct game command (exits).
    Command(String),
}

/// One focusable entity in the current room.
#[derive(Clone, Debug, PartialEq)]
pub struct InteractEntity {
    /// Display / speech label (e.g. "a muddy hog (stunned)").
    pub label: String,
    /// Stable identity across list rebuilds: exist id or exit direction.
    pub key: String,
    pub action: InteractAction,
}

/// Compass short code -> movement command word.
fn exit_command(dir: &str) -> &str {
    match dir {
        "n" => "north",
        "ne" => "northeast",
        "e" => "east",
        "se" => "southeast",
        "s" => "south",
        "sw" => "southwest",
        "w" => "west",
        "nw" => "northwest",
        other => other, // up, down, out arrive as full words
    }
}

/// Fallback noun for entities whose feed omitted one: last word of the
/// display name (menus want "hog", not "a muddy hog").
fn fallback_noun(name: &str) -> String {
    name.rsplit(' ').next().unwrap_or(name).to_string()
}

impl AppCore {
    /// Toggle interact mode (keybind action `interact_mode`).
    pub fn toggle_interact_mode(&mut self) {
        if self.ui_state.input_mode == InputMode::Interact {
            self.exit_interact_mode();
        } else {
            self.enter_interact_mode();
        }
    }

    pub fn enter_interact_mode(&mut self) {
        let Some(category) = self.first_populated_category() else {
            self.add_system_message("Interact: nothing here to interact with.");
            return;
        };
        self.ui_state.interact = Some(InteractState {
            category,
            index: 0,
            focus_key: None,
        });
        self.ui_state.input_mode = InputMode::Interact;
        self.sync_interact_focus();
        self.announce_interact_focus();
        self.needs_render = true;
    }

    pub fn exit_interact_mode(&mut self) {
        self.ui_state.interact = None;
        if self.ui_state.input_mode == InputMode::Interact {
            self.ui_state.input_mode = InputMode::Normal;
        }
        self.needs_render = true;
    }

    /// Entities for one category, in display order.
    pub fn interact_entities(&self, category: InteractCategory) -> Vec<InteractEntity> {
        match category {
            InteractCategory::Creatures => self
                .game_state
                .room_creatures
                .iter()
                .map(|c| {
                    let statuses = c.display_statuses();
                    let label = if statuses.is_empty() {
                        c.name.clone()
                    } else {
                        format!("{} ({})", c.name, statuses.join(", "))
                    };
                    let exist = c.id.trim_start_matches('#').to_string();
                    InteractEntity {
                        label,
                        key: exist.clone(),
                        action: InteractAction::Menu {
                            exist_id: exist,
                            noun: c.noun.clone().unwrap_or_else(|| fallback_noun(&c.name)),
                        },
                    }
                })
                .collect(),
            InteractCategory::Objects => self
                .game_state
                .room_objects
                .iter()
                .map(|o| {
                    let exist = o.id.trim_start_matches('#').to_string();
                    InteractEntity {
                        label: o.name.clone(),
                        key: exist.clone(),
                        action: InteractAction::Menu {
                            exist_id: exist,
                            noun: o.noun.clone().unwrap_or_else(|| fallback_noun(&o.name)),
                        },
                    }
                })
                .collect(),
            InteractCategory::Players => self
                .game_state
                .room_players
                .iter()
                .map(|p| {
                    let exist = p.id.trim_start_matches('#').to_string();
                    InteractEntity {
                        label: p.name.clone(),
                        key: exist.clone(),
                        action: InteractAction::Menu {
                            exist_id: exist,
                            noun: p.name.clone(),
                        },
                    }
                })
                .collect(),
            InteractCategory::Exits => self
                .game_state
                .compass_dirs
                .iter()
                .map(|dir| {
                    let command = exit_command(dir).to_string();
                    InteractEntity {
                        label: command.clone(),
                        key: dir.clone(),
                        action: InteractAction::Command(command),
                    }
                })
                .collect(),
        }
    }

    fn first_populated_category(&self) -> Option<InteractCategory> {
        InteractCategory::ORDER
            .into_iter()
            .find(|cat| !self.interact_entities(*cat).is_empty())
    }

    /// Re-resolve the stored focus against current entity lists: find the
    /// remembered key, else clamp the index into range. Call before any
    /// read so room churn between frames can't leave a stale index.
    fn sync_interact_focus(&mut self) {
        let Some(state) = self.ui_state.interact.clone() else {
            return;
        };
        let entities = self.interact_entities(state.category);
        let Some(interact) = self.ui_state.interact.as_mut() else {
            return;
        };
        if entities.is_empty() {
            interact.index = 0;
            interact.focus_key = None;
            return;
        }
        if let Some(found) = state
            .focus_key
            .as_deref()
            .and_then(|key| entities.iter().position(|e| e.key == key))
        {
            interact.index = found;
        } else {
            interact.index = state.index.min(entities.len() - 1);
            interact.focus_key = Some(entities[interact.index].key.clone());
        }
    }

    /// Currently focused entity with its position: (category, index, count, entity).
    pub fn interact_current(&self) -> Option<(InteractCategory, usize, usize, InteractEntity)> {
        let state = self.ui_state.interact.as_ref()?;
        let entities = self.interact_entities(state.category);
        let entity = entities.get(state.index)?.clone();
        Some((state.category, state.index, entities.len(), entity))
    }

    /// Exist id of the focused entity, for frontend focus-ring rendering.
    /// None for exits (they have no exist id) or when the mode is off.
    pub fn interact_focus_exist_id(&self) -> Option<String> {
        let (category, _, _, entity) = self.interact_current()?;
        match category {
            InteractCategory::Exits => None,
            _ => Some(entity.key),
        }
    }

    /// Move focus within the category (+1 next / -1 prev), wrapping.
    pub fn interact_move(&mut self, delta: i32) {
        self.sync_interact_focus();
        let Some(state) = self.ui_state.interact.clone() else {
            return;
        };
        let entities = self.interact_entities(state.category);
        if entities.is_empty() {
            self.announce_interact("nothing here");
            return;
        }
        let len = entities.len() as i32;
        let next = (state.index as i32 + delta).rem_euclid(len) as usize;
        if let Some(interact) = self.ui_state.interact.as_mut() {
            interact.index = next;
            interact.focus_key = Some(entities[next].key.clone());
        }
        self.announce_interact_focus();
        self.needs_render = true;
    }

    /// Switch category (+1 / -1), skipping empty ones, wrapping.
    pub fn interact_category_move(&mut self, delta: i32) {
        let Some(state) = self.ui_state.interact.clone() else {
            return;
        };
        let order = InteractCategory::ORDER;
        let start = order.iter().position(|c| *c == state.category).unwrap_or(0) as i32;
        let len = order.len() as i32;
        for step in 1..=len {
            let candidate = order[((start + delta * step).rem_euclid(len)) as usize];
            if candidate == state.category {
                continue;
            }
            if !self.interact_entities(candidate).is_empty() {
                if let Some(interact) = self.ui_state.interact.as_mut() {
                    interact.category = candidate;
                    interact.index = 0;
                    interact.focus_key = None;
                }
                self.sync_interact_focus();
                self.announce_interact_focus();
                self.needs_render = true;
                return;
            }
        }
        self.announce_interact("no other categories");
    }

    /// Activate the focused entity. Returns the outbound command for the
    /// network layer (same forms `handle_link_click` dispatches): a
    /// `_menu` request for entities, a bare movement command for exits.
    /// `click_pos` anchors where the frontend wants the menu to open.
    pub fn interact_activate(&mut self, click_pos: (u16, u16)) -> Option<String> {
        self.sync_interact_focus();
        let (_, _, _, entity) = self.interact_current()?;
        match entity.action {
            InteractAction::Menu { exist_id, noun } => {
                Some(self.request_menu(exist_id, noun, click_pos))
            }
            InteractAction::Command(command) => {
                // Walking an exit leaves the room; drop out of the mode so
                // stale focus doesn't linger over the next room's entities.
                self.exit_interact_mode();
                Some(command)
            }
        }
    }

    /// Fill `<target_id>` / `<target_noun>` in a macro from the focused
    /// interact entity, so a bound `target #<target_id>\rincant 611\r`
    /// attacks whatever the focus ring is on. Text without placeholders
    /// passes through untouched; None means the macro needs a target but
    /// none is focused (mode off, empty category, or an exit) — callers
    /// drop the command instead of sending the literal placeholder.
    pub fn substitute_interact_placeholders(&self, text: String) -> Option<String> {
        if !text.contains("<target_id>") && !text.contains("<target_noun>") {
            return Some(text);
        }
        let (_, _, _, entity) = self.interact_current()?;
        let InteractAction::Menu { exist_id, noun } = entity.action else {
            return None; // exits have no exist id
        };
        Some(
            text.replace("<target_id>", &exist_id)
                .replace("<target_noun>", &noun),
        )
    }

    /// One-line summary for a status overlay:
    /// "Creatures 2/5: a muddy hog (stunned)".
    pub fn interact_overlay_line(&self) -> Option<String> {
        let (category, index, count, entity) = self.interact_current()?;
        Some(format!(
            "{} {}/{}: {}",
            category.label(),
            index + 1,
            count,
            entity.label
        ))
    }

    fn announce_interact_focus(&mut self) {
        if let Some((category, _, _, entity)) = self.interact_current() {
            let text = format!("{}, {}", entity.label, category.label());
            self.announce_interact(&text);
        }
    }

    /// Speak interact-mode feedback when TTS is on (focus changes are
    /// ephemeral UI, not game text — bypass the queue).
    fn announce_interact(&mut self, text: &str) {
        if self.tts_manager.is_enabled() {
            if let Err(err) = self.tts_manager.speak_text_now(text) {
                tracing::debug!("interact TTS announce failed: {}", err);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::{Creature, Player, RoomObject};

    fn core_with_room() -> AppCore {
        let mut core = AppCore::new_for_test();
        core.game_state.room_creatures = vec![
            Creature {
                name: "a muddy hog".into(),
                noun: Some("hog".into()),
                id: "#111".into(),
                status: None,
                flags: None,
            },
            Creature {
                name: "a kobold".into(),
                noun: Some("kobold".into()),
                id: "#222".into(),
                status: Some("stunned".into()),
                flags: None,
            },
        ];
        core.game_state.room_objects = vec![RoomObject {
            name: "a silver ring".into(),
            noun: Some("ring".into()),
            id: "333".into(),
        }];
        core.game_state.room_players = vec![Player {
            name: "Nisugi".into(),
            id: "-444".into(),
            primary_status: None,
            secondary_status: None,
            dead: false,
        }];
        core.game_state.compass_dirs = vec!["n".into(), "sw".into(), "out".into()];
        core
    }

    #[test]
    fn enter_picks_first_populated_category() {
        let mut core = core_with_room();
        core.enter_interact_mode();
        let (cat, index, count, entity) = core.interact_current().unwrap();
        assert_eq!(cat, InteractCategory::Creatures);
        assert_eq!((index, count), (0, 2));
        assert_eq!(entity.key, "111");
        assert_eq!(core.ui_state.input_mode, InputMode::Interact);
    }

    #[test]
    fn enter_skips_empty_categories() {
        let mut core = core_with_room();
        core.game_state.room_creatures.clear();
        core.game_state.room_objects.clear();
        core.enter_interact_mode();
        let (cat, _, _, _) = core.interact_current().unwrap();
        assert_eq!(cat, InteractCategory::Players);
    }

    #[test]
    fn enter_with_empty_room_stays_in_normal_mode() {
        let mut core = AppCore::new_for_test();
        core.enter_interact_mode();
        assert_eq!(core.ui_state.input_mode, InputMode::Normal);
        assert!(core.ui_state.interact.is_none());
    }

    #[test]
    fn move_wraps_and_tracks_key() {
        let mut core = core_with_room();
        core.enter_interact_mode();
        core.interact_move(1);
        let (_, index, _, entity) = core.interact_current().unwrap();
        assert_eq!(index, 1);
        assert_eq!(entity.key, "222");
        core.interact_move(1); // wraps
        assert_eq!(core.interact_current().unwrap().1, 0);
        core.interact_move(-1); // wraps backwards
        assert_eq!(core.interact_current().unwrap().1, 1);
    }

    #[test]
    fn focus_follows_entity_across_list_rewrite() {
        let mut core = core_with_room();
        core.enter_interact_mode();
        core.interact_move(1); // focus kobold (#222)
                               // Room update: a new creature is prepended (indices shift)
        core.game_state.room_creatures.insert(
            0,
            Creature {
                name: "a rolton".into(),
                noun: Some("rolton".into()),
                id: "#555".into(),
                status: None,
                flags: None,
            },
        );
        core.interact_move(1); // should step from the kobold, now at index 2
        let (_, index, count, entity) = core.interact_current().unwrap();
        assert_eq!(count, 3);
        assert_eq!((index, entity.key.as_str()), (0, "555")); // wrapped past end
    }

    #[test]
    fn focus_clamps_when_entity_vanishes() {
        let mut core = core_with_room();
        core.enter_interact_mode();
        core.interact_move(1); // kobold, index 1
        core.game_state.room_creatures.pop(); // kobold leaves
        core.interact_move(1); // sync clamps to index 0, then wraps to 0 (len 1)
        let (_, index, count, _) = core.interact_current().unwrap();
        assert_eq!((index, count), (0, 1));
    }

    #[test]
    fn category_move_skips_empty_and_wraps() {
        let mut core = core_with_room();
        core.game_state.room_objects.clear();
        core.enter_interact_mode();
        core.interact_category_move(1); // Objects empty -> Players
        assert_eq!(
            core.interact_current().unwrap().0,
            InteractCategory::Players
        );
        core.interact_category_move(-1); // back over Objects -> Creatures
        assert_eq!(
            core.interact_current().unwrap().0,
            InteractCategory::Creatures
        );
    }

    #[test]
    fn activate_entity_requests_menu_without_double_hash() {
        let mut core = core_with_room();
        core.enter_interact_mode();
        let outbound = core.interact_activate((10, 10)).unwrap();
        assert!(outbound.starts_with("_menu #111 "), "got: {outbound}");
        // Mode stays active for menu activation
        assert_eq!(core.ui_state.input_mode, InputMode::Interact);
    }

    #[test]
    fn activate_exit_sends_direction_and_exits_mode() {
        let mut core = core_with_room();
        core.game_state.room_creatures.clear();
        core.game_state.room_objects.clear();
        core.game_state.room_players.clear();
        core.enter_interact_mode();
        assert_eq!(core.interact_current().unwrap().0, InteractCategory::Exits);
        core.interact_move(1); // "sw"
        let outbound = core.interact_activate((0, 0)).unwrap();
        assert_eq!(outbound, "southwest");
        assert_eq!(core.ui_state.input_mode, InputMode::Normal);
        assert!(core.ui_state.interact.is_none());
    }

    #[test]
    fn overlay_line_and_statuses() {
        let mut core = core_with_room();
        core.enter_interact_mode();
        core.interact_move(1);
        assert_eq!(
            core.interact_overlay_line().unwrap(),
            "Creatures 2/2: a kobold (stunned)"
        );
    }

    #[test]
    fn placeholders_fill_from_interact_focus() {
        let mut core = core_with_room();
        core.enter_interact_mode(); // Creatures, hog #111
        assert_eq!(
            core.substitute_interact_placeholders(
                "target #<target_id>\rincant 611\rhide".to_string()
            ),
            Some("target #111\rincant 611\rhide".to_string())
        );
        assert_eq!(
            core.substitute_interact_placeholders("whisper <target_noun> hi".to_string()),
            Some("whisper hog hi".to_string())
        );
        // No placeholders: untouched, even with the mode off.
        core.exit_interact_mode();
        assert_eq!(
            core.substitute_interact_placeholders("look\r".to_string()),
            Some("look\r".to_string())
        );
        // Placeholder without a focus: dropped, not sent literally.
        assert_eq!(
            core.substitute_interact_placeholders("target #<target_id>".to_string()),
            None
        );
    }

    #[test]
    fn placeholders_refuse_exits() {
        let mut core = core_with_room();
        core.game_state.room_creatures.clear();
        core.game_state.room_objects.clear();
        core.game_state.room_players.clear();
        core.enter_interact_mode(); // Exits only
        assert_eq!(
            core.substitute_interact_placeholders("target #<target_id>".to_string()),
            None
        );
    }

    #[test]
    fn toggle_exits_cleanly() {
        let mut core = core_with_room();
        core.toggle_interact_mode();
        assert_eq!(core.ui_state.input_mode, InputMode::Interact);
        core.toggle_interact_mode();
        assert_eq!(core.ui_state.input_mode, InputMode::Normal);
        assert!(core.ui_state.interact.is_none());
    }
}
