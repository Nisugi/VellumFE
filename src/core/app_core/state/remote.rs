//! Remote (web/phone) client integration: sink attachment, wheel and
//! macro pushes, session control, state flushes, and remote map views.

use super::*;

impl AppCore {
    /// Returns a list of commands to send to the server (for macros)

    /// Execute a KeyAction (dispatch to the appropriate method)

    /// Attach the remote client sink (web frontend sidecar).
    /// Called by the runtime after it spawns the web server task.
    pub fn enable_remote(&mut self, mut sink: crate::core::remote::RemoteSink) {
        sink.set_macros(&self.config.macros);
        sink.set_wheels(&self.config);
        self.message_processor.remote = Some(sink);
    }

    /// The port our own web sidecar actually bound, once it has.
    ///
    /// The configured port is only a starting point -- `serve` walks upward
    /// when it is taken, which is exactly what happens when several instances
    /// run on one machine. The multi-account hub needs the real one so it can
    /// skip dialing itself.
    pub fn remote_bound_port(&self) -> Option<u16> {
        self.message_processor
            .remote
            .as_ref()
            .and_then(|sink| sink.bound_port())
    }

    /// The authenticated endpoint installed by our web sidecar. Port and
    /// token originate from the serving task as one indivisible readiness
    /// value, so launchers cannot advertise mismatched auth state.
    pub fn remote_launch_endpoint(&self) -> Option<crate::core::remote::RemoteLaunchEndpoint> {
        self.message_processor
            .remote
            .as_ref()
            .and_then(|sink| sink.launch_endpoint())
    }

    /// Re-publish radial-wheel definitions to remote clients after the
    /// wheel config changed (keybinds reload, desktop wheel editor).
    /// No-op when web is disabled.
    pub fn push_remote_wheels(&mut self) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_wheels(&self.config);
        }
    }

    /// Surface per-wheel `button` conflicts against the `[controller]`
    /// table (the runtime binding authority). Two classes are reported:
    ///   - a wheel's `button` disagrees with the button actually bound to
    ///     its `controller_wheel[:name]` action — `[controller]` wins, so
    ///     the note tells the user which button really opens the wheel;
    ///   - two wheels claim the same `button` — only one can win.
    /// Called after controller config (re)loads. Silent when clean. These
    /// are wheel-config validation results, not gameplay output: they go
    /// to the log, and the `.controller` editor's Wheels tab shows them
    /// inline (`wheel_binding_conflicts`) next to the controls they
    /// concern — never into the story window.
    pub fn warn_wheel_binding_conflicts(&mut self) {
        for w in Self::wheel_binding_conflicts(&self.config) {
            tracing::warn!("{}", w);
        }
    }

    /// The current wheel↔button conflict list, computed fresh from config.
    /// Pure so the controller editor can render it live each frame (edits
    /// from either surface — binding dialog or wheel editor — re-validate
    /// in place).
    pub fn wheel_binding_conflicts(config: &crate::config::Config) -> Vec<String> {
        use crate::config::KeyBindAction;
        // button -> wheel key ("" = default) from [controller].
        let mut bound: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (button, action) in &config.controller_binds {
            if let KeyBindAction::Action(name) = action {
                let key = if name == "controller_wheel" {
                    Some(String::new())
                } else {
                    name.strip_prefix("controller_wheel:").map(str::to_string)
                };
                if let Some(key) = key {
                    bound.insert(button.clone(), key);
                }
            }
        }

        let mut warnings: Vec<String> = Vec::new();
        // Meta button vs [controller] authority.
        let mut claimed: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (name, meta) in &config.controller_wheels_meta {
            let Some(button) = meta.button.as_deref() else {
                continue;
            };
            // Wheel key as [controller] would encode it ("default" wheel
            // binds the bare controller_wheel action = key "").
            let wheel_key = if name == "default" { "" } else { name.as_str() };
            match bound.get(button) {
                Some(k) if k == wheel_key => {} // agrees
                Some(other) => {
                    let other_label = if other.is_empty() { "default" } else { other };
                    warnings.push(format!(
                        "Wheel '{}' lists button '{}', but [controller] binds '{}' to the '{}' wheel — [controller] wins.",
                        name, button, button, other_label
                    ));
                }
                None => warnings.push(format!(
                    "Wheel '{}' lists button '{}', but nothing in [controller] opens it — bind '{}' to 'controller_wheel:{}'.",
                    name, button, button, name
                )),
            }
            if let Some(prev) = claimed.insert(button.to_string(), name.clone()) {
                warnings.push(format!(
                    "Wheels '{}' and '{}' both claim button '{}' — only one can open on it.",
                    prev, name, button
                ));
            }
        }

        warnings
    }

    /// Surface `span` problems across every configured wheel (default +
    /// named, recursing into folders): spans that sum over 360°, resolve
    /// below the minimum width, or leave a ring unable to close. Advisory
    /// only — the runtime resolver always produces a usable ring by
    /// clamping and scaling; these tell the user their numbers were
    /// adjusted. Called alongside `warn_wheel_binding_conflicts` on load
    /// and editor save. The dynamic portals wheel carries no spans, so it
    /// is skipped.
    pub fn warn_wheel_span_conflicts(&mut self) {
        use crate::config::validate_wheel_spans;
        let mut issues = validate_wheel_spans("default", &self.config.controller_wheel);
        for (name, slices) in &self.config.controller_wheels {
            if name == Self::PORTAL_WHEEL_KEY {
                continue;
            }
            issues.extend(validate_wheel_spans(name, slices));
        }
        for issue in issues {
            self.add_system_message(&issue.message());
        }
    }

    /// Reserved dynamic wheel name: slices are built from the current
    /// room's portal list at open time instead of TOML.
    pub const PORTAL_WHEEL_KEY: &str = "portals";

    /// Slices for a wheel key: the dynamic portals wheel first
    /// (shadowing any static wheel of that name), else the static
    /// config lookup. Owned — dynamic slices have no home in config.
    pub fn wheel_slices(
        &self,
        key: &str,
        path: &[usize],
    ) -> Option<Vec<crate::config::WheelSlice>> {
        if key == Self::PORTAL_WHEEL_KEY {
            if !path.is_empty() {
                return None; // flat wheel: portals have no folders
            }
            let slices: Vec<crate::config::WheelSlice> = self
                .portal_candidate_list()
                .into_iter()
                .map(|c| crate::config::WheelSlice {
                    // The wedge shows the movement label (verb-stripped for a
                    // plain "go gate" -> "gate"); a StringProc edge's label is
                    // already the movement (e.g. "climb footpath"). The pick
                    // runs c.command (a .go2 <id> for proc edges).
                    label: c
                        .label
                        .split_once(' ')
                        .map(|(_, rest)| rest.to_string())
                        .unwrap_or_else(|| c.label.clone()),
                    command: c.command,
                    // Dynamic slices carry no span/inner/color: the portals
                    // ring stays evenly spaced with the global dead zone.
                    ..Default::default()
                })
                .collect();
            return (!slices.is_empty()).then_some(slices);
        }
        self.config.wheel_level_slices(key, path).cloned()
    }

    /// Resolve a wheel pick (remote clients): the dynamic portals wheel by
    /// index, else static config. `<target_id>`/`<target_noun>` resolve
    /// against the host's interact focus so a phone combat wheel casts at
    /// the selected creature; a placeholder with nothing focused yields
    /// None (the pick is dropped, never sent literally) — mirroring the
    /// GUI wheel and bound interact macros. The GUI's own release-fire
    /// substitutes in wheel_fire, so it doesn't route through here.
    pub fn wheel_pick_command(&self, key: &str, path: &[usize]) -> Option<String> {
        let raw = if key == Self::PORTAL_WHEEL_KEY {
            let (&leaf, folders) = path.split_last()?;
            if !folders.is_empty() {
                return None;
            }
            self.portal_commands().into_iter().nth(leaf)?
        } else {
            self.config.wheel_pick_command(key, path)?
        };
        self.substitute_interact_placeholders(raw)
    }

    /// Declare that this runtime accepts session control (Connect /
    /// Disconnect) from web clients. Only the headless runtime does.
    pub fn set_remote_session_control(&mut self, enabled: bool) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_session_control(enabled);
        }
    }

    /// Push a session status change to remote clients (headless supervisor
    /// state transitions). No-op when web is disabled.
    pub fn set_remote_session_state(&mut self, info: crate::core::remote::RemoteSessionInfo) {
        use crate::core::remote::SessionState;
        use crate::core::session_registry::SessionLifecycleState as RegistryState;
        crate::core::session_registry::set_lifecycle(match info.state {
            SessionState::Idle => RegistryState::Idle,
            SessionState::Authenticating | SessionState::Connecting => RegistryState::Connecting,
            SessionState::Connected => RegistryState::Connected,
            SessionState::Reconnecting => RegistryState::Reconnecting,
            SessionState::Disconnected => RegistryState::Disconnected,
        });
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_session_state(info);
        }
    }

    /// Broadcast a highlight-triggered sound to remote clients, which play
    /// it via the browser (used by the headless runtime where there is no
    /// native audio device). No-op when web is disabled.
    pub fn push_remote_sound(&mut self, file: &str, volume: Option<f32>) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_sound(file, volume);
        }
    }

    /// Create or edit a phone-authored macro button. The edit lands in the
    /// macros-local.toml overlay (the hand-written macros.toml is never
    /// rewritten), then the merged set is re-published to every client.
    pub fn apply_macro_save(
        &mut self,
        group: Option<String>,
        button: crate::config::MacroButton,
        original: Option<(Option<String>, String)>,
    ) {
        let has_command = !button.command.as_deref().unwrap_or("").trim().is_empty();
        let has_client = !button.client.as_deref().unwrap_or("").trim().is_empty();
        if button.label.trim().is_empty()
            || (!has_command && !has_client && button.options.is_empty())
        {
            self.add_system_message(
                "Macro not saved: a label plus a command, app action, or menu options are required",
            );
            return;
        }
        let label = button.label.clone();
        self.config.macros_local.upsert_button(
            group.as_deref(),
            button,
            original
                .as_ref()
                .map(|(group, label)| (group.as_deref(), label.as_str())),
        );
        self.persist_and_push_macros(&format!("Saved macro '{}'", label));
    }

    /// Delete a phone-authored macro button. Buttons from the hand-written
    /// macros.toml are not deletable remotely.
    pub fn apply_macro_delete(&mut self, group: Option<String>, label: String) {
        if self
            .config
            .macros_local
            .delete_button(group.as_deref(), &label)
        {
            self.persist_and_push_macros(&format!("Deleted macro '{}'", label));
        } else {
            self.add_system_message(&format!(
                "Macro '{}' is defined in macros.toml and can only be edited there",
                label
            ));
        }
    }

    pub(super) fn persist_and_push_macros(&mut self, message: &str) {
        if let Err(e) = self
            .config
            .macros_local
            .save_local(self.config.character.as_deref())
        {
            self.add_system_message(&format!("Failed to save macros-local.toml: {e:#}"));
            return;
        }
        let base = crate::config::MacrosConfig::load_base(self.config.character.as_deref())
            .unwrap_or_default();
        self.config.macros =
            crate::config::MacrosConfig::merge(base, self.config.macros_local.clone());
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_macros(&self.config.macros);
        }
        self.add_system_message(message);
    }

    /// Flush coalesced game-state deltas to remote clients. Called once
    /// per message batch by the frontend loop; no-op when web is disabled.
    pub fn flush_remote_state(&mut self) {
        if self.message_processor.remote.is_none() {
            return;
        }
        let mut snap = crate::core::remote::RemoteStateSnapshot::from_game_state(
            &self.game_state,
            &self.config.target_list.excluded_nouns,
        );
        // Room identity lives on AppCore, not GameState. Under Lich the
        // decorated header supplies the map room number while `<nav rm>`
        // supplies a distinct navigation UID; preserve both instead of
        // presenting the UID as though it were the room number.
        if snap.room_id.is_none() {
            snap.room_id = crate::core::messages::canonical_room_id(
                self.lich_room_id.as_deref(),
                self.nav_room_id.as_deref(),
            );
        }
        // Portal resolution needs the map service, which lives here.
        snap.portals = self.portal_commands();
        // Doll variants came from the legacy live-manifest skin runtime;
        // pool dolls carry none, so the snapshot keeps its default
        // (no variant, nothing hidden) and phone clients draw the base set.
        // Creature field: host-placed cards on the solver's virtual stage,
        // in draw order, so browsers paint the list as-is with no solver
        // and no condition logic of their own (the doll rule, again).
        snap.field = self.build_remote_field();
        // Real sessions rarely set game_state.room_name/exits; fall back
        // the same way the room widget does (see gui sync_room_windows):
        // subtitle from <streamWindow> for the name, compass for exits.
        if snap
            .room_name
            .as_deref()
            .is_none_or(|n| n.trim().is_empty())
        {
            snap.room_name = self.room_subtitle.as_ref().map(|subtitle| {
                subtitle
                    .trim()
                    .trim_start_matches('-')
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string()
            });
        }
        if snap.exits.is_empty() {
            snap.exits = self.game_state.compass_dirs.clone();
        }
        if snap.character.is_none() {
            // connection.character comes from config.toml; config.character
            // is the CLI --character/--profile name.
            snap.character = self
                .config
                .connection
                .character
                .clone()
                .or_else(|| self.config.character.clone());
        }
        // The map lives on AppCore, not GameState: overlay the drawable
        // scene + position for the phone's map view.
        let (map_scene, map_state) = self.build_remote_map();
        snap.map_scene = map_scene;
        snap.map_state = map_state;
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.flush_state(snap);
        }
    }

    /// Creature-field cards for remote clients: one per creature, placed
    /// on the solver's fixed 880x470 virtual stage, farthest first.
    fn build_remote_field(&self) -> Vec<crate::core::remote::RemoteFieldCard> {
        let field = &self.creature_field;
        if field.units().is_empty() {
            return Vec::new();
        }
        let current = self
            .game_state
            .target_list
            .current_target
            .trim_start_matches('#');
        let mut out = Vec::new();
        for &i in &field.draw_order() {
            let unit = &field.units()[i];
            let rect = field.rect(unit);
            let (fx, fy) = field.foot(unit);
            for member in &unit.members {
                let Some(c) = self
                    .game_state
                    .room_creatures
                    .iter()
                    .find(|c| &c.id == member)
                else {
                    continue;
                };
                let flags = c.flags.as_ref();
                let lift = flags.and_then(|f| {
                    if f.has_flag("flying") {
                        Some(-0.22)
                    } else if f.has_flag("hovering") {
                        Some(-0.12)
                    } else {
                        None
                    }
                });
                out.push(crate::core::remote::RemoteFieldCard {
                    id: member.trim_start_matches('#').to_string(),
                    noun: c.noun.clone().unwrap_or_else(|| {
                        c.name.rsplit(' ').next().unwrap_or(&c.name).to_string()
                    }),
                    name: c.name.clone(),
                    rect: [rect.x0, rect.y0, rect.x1, rect.y1],
                    foot: [fx, fy],
                    dead: c.is_dead(),
                    boss: flags.is_some_and(|f| f.is_boss()),
                    current: !current.is_empty() && member.trim_start_matches('#') == current,
                    statuses: flags.map(|f| f.statuses.clone()).unwrap_or_default(),
                    lift,
                });
            }
        }
        out
    }

    /// The phone map's wire data: the same sheet + building filter the
    /// desktop mini map draws, cached until the drawn view changes, plus
    /// the small per-step position/ghost state.
    pub(super) fn build_remote_map(
        &mut self,
    ) -> (
        crate::core::remote::RemoteMapSceneRef,
        crate::core::remote::RemoteMapState,
    ) {
        use crate::core::layout_engine::Sheet;
        use crate::core::remote::{
            RemoteClassicMap, RemoteGhostEdge, RemoteGhostNode, RemoteMapSceneRef, RemoteMapState,
        };

        let map = &self.map;
        let mut state = RemoteMapState::default();
        state.classic = map
            .current_classic_map()
            .map(|(image, room_rect)| RemoteClassicMap { image, room_rect });
        let Some(scene) = map.current_scene() else {
            self.remote_map_cache = None;
            return (RemoteMapSceneRef::default(), state);
        };

        let current = map.current_room_id;
        let (sheet, center, filter) = match current.and_then(|id| scene.room(id)) {
            Some((sheet, room)) => (
                sheet,
                Some(room.cell),
                (sheet == Sheet::Interiors).then(|| scene.cluster_groups(room.group)),
            ),
            None => (Sheet::Outdoor, None, None),
        };

        // Ghost sketch overlay (session-only unmapped interiors); rendered
        // only in cartography mode — everyday play shows mapdb truth.
        let overlay = (self.config.map.mapping_mode && !map.ghosts().is_empty()).then(|| {
            crate::core::ghost_rooms::build_overlay(map.ghosts(), scene, sheet, filter.as_ref())
        });
        let ghost_cell = map
            .current_ghost
            .and_then(|uid| overlay.as_ref()?.cell_of(uid));

        state.available = true;
        state.location = map
            .current_location
            .as_deref()
            .map(|key| map.display_name(key).to_owned());
        state.room = current;
        state.cell = ghost_cell.or(center).map(|c| [c.x, c.y]);
        state.in_ghost = ghost_cell.is_some();
        state.travel = self.travel.task().and_then(|task| {
            let db = map.mapdb()?;
            let from = current?;
            Some(crate::core::remote::RemoteTravelStatus {
                dest: task.destination,
                done: task.rooms_total().saturating_sub(task.rooms_remaining()),
                total: task.rooms_total(),
                eta: crate::core::travel::format_eta(task.eta_seconds(db, from)),
            })
        });
        if let Some(overlay) = &overlay {
            let current_ghost = map.current_ghost;
            state.ghosts = overlay
                .nodes
                .iter()
                .map(|n| RemoteGhostNode {
                    x: n.cell.x,
                    y: n.cell.y,
                    cur: current_ghost == Some(n.uid),
                })
                .collect();
            state.ghost_edges = overlay
                .edges
                .iter()
                .map(|e| RemoteGhostEdge {
                    x1: e.a.x,
                    y1: e.a.y,
                    x2: e.b.x,
                    y2: e.b.y,
                    l: e.label.clone(),
                })
                .collect();
        }

        // Scene: rebuild only when the drawn view changes (location/sheet/
        // building or a layout regeneration — the Arc pointer covers all).
        let cluster_key = filter
            .as_ref()
            .map(|set| set.iter().min().copied().unwrap_or(0));
        let key = (std::sync::Arc::as_ptr(scene) as usize, sheet, cluster_key);
        if let Some((cached_key, cached)) = &self.remote_map_cache {
            if *cached_key == key {
                return (RemoteMapSceneRef(Some(cached.clone())), state);
            }
        }

        let wire = std::sync::Arc::new(wire_map_scene(scene, sheet, filter.as_ref()));
        self.remote_map_cache = Some((key, wire.clone()));
        (RemoteMapSceneRef(Some(wire)), state)
    }

    /// Location list for a phone's map picker.
    pub fn handle_remote_map_locations(&mut self, client_id: u64, request_id: u64) {
        // With curated membership: map keys, curated first (the phone shows
        // them verbatim; satellite keys are functional placeholders until
        // the satellite set gets curated down). Fallback: locations.
        let locations: Vec<String> = match self.map.membership() {
            Some(membership) => membership
                .list_maps()
                .into_iter()
                .map(|(key, _, _, _)| key)
                .collect(),
            None => self
                .map
                .mapdb()
                .map(|db| db.locations().map(str::to_owned).collect())
                .unwrap_or_default(),
        };
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_map_locations(client_id, request_id, locations);
        }
    }

    /// A phone wants to browse another location's map. Layout generation is
    /// async: reply now when the scene is cached, otherwise queue and let
    /// `poll_map` answer when the worker finishes.
    pub fn handle_remote_map_view(&mut self, client_id: u64, request_id: u64, location: String) {
        let known = self
            .map
            .membership()
            .is_some_and(|m| m.rooms_of_map(&location).is_some())
            || self
                .map
                .mapdb()
                .map(|db| db.rooms(&location).is_some())
                .unwrap_or(false);
        if !known {
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_map_browse(
                    client_id,
                    request_id,
                    location.clone(),
                    None,
                    Some(format!("'{location}' is not in the map database")),
                );
            }
            return;
        }
        self.map.request_location(&location);
        self.pending_map_views
            .push((client_id, request_id, location));
        self.service_pending_map_views();
    }

    /// Answer browse requests whose layouts have finished generating.
    pub(super) fn service_pending_map_views(&mut self) {
        if self.pending_map_views.is_empty() {
            return;
        }
        let mut still_pending = Vec::new();
        for (client_id, request_id, location) in std::mem::take(&mut self.pending_map_views) {
            let Some(scene) = self.map.scene_for(&location) else {
                still_pending.push((client_id, request_id, location));
                continue;
            };
            // Browse the outdoor sheet; interior-only locations fall back
            // to their interiors shelf.
            let sheet = if scene.outdoor.rooms.is_empty() {
                crate::core::layout_engine::Sheet::Interiors
            } else {
                crate::core::layout_engine::Sheet::Outdoor
            };
            let wire = std::sync::Arc::new(wire_map_scene(scene, sheet, None));
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_map_browse(client_id, request_id, location, Some(wire), None);
            }
        }
        self.pending_map_views = still_pending;
    }
}
