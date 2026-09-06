//! Per-frame pump of parsed server messages out of the network channel
//! and into AppCore, plus the remote web-client command path.

use super::*;

impl VellumGuiApp {
    pub(super) fn pump_server_messages(&mut self) {
        // Commands from remote web clients run the same dispatch path as
        // the local input bar.
        while let Ok(event) = self.remote_rx.try_recv() {
            match event {
                crate::core::remote::RemoteEvent::Command { client_id, text } => {
                    tracing::debug!("remote command: '{}'", text);
                    self.record_command_history(&text);
                    self.dispatch_remote_command(client_id, text);
                }
                crate::core::remote::RemoteEvent::LinkTap {
                    client_id,
                    request_id,
                    exist_id,
                    noun,
                    text,
                    coord,
                } => {
                    // Resolved exactly like a local click: <d>/coord links
                    // become direct commands, plain links a _menu request
                    // tagged to route back to this client.
                    let link = crate::data::LinkData {
                        exist_id,
                        noun,
                        text,
                        coord,
                    };
                    if let Some(cmd) = self.app_core.resolve_link_activation(
                        &link,
                        crate::core::remote::MenuOrigin::Remote {
                            client_id,
                            request_id,
                        },
                    ) {
                        self.app_core
                            .perf_stats
                            .record_bytes_sent((cmd.len() + 1) as u64);
                        let sent = self.command_tx.send(cmd.clone()).is_ok();
                        self.app_core.finish_game_command_send(&cmd, sent);
                    }
                }
                crate::core::remote::RemoteEvent::MacroSave {
                    group,
                    label,
                    command,
                    color,
                    confirm,
                    insert,
                    client,
                    options,
                    original,
                } => {
                    let button = crate::config::MacroButton {
                        label,
                        // A client-action button carries no game command.
                        command: Some(command)
                            .filter(|c| !c.is_empty())
                            .filter(|_| client.is_none()),
                        client,
                        color,
                        confirm,
                        insert,
                        options,
                        ..Default::default()
                    };
                    self.app_core.apply_macro_save(group, button, original);
                }
                crate::core::remote::RemoteEvent::MacroDelete { group, label } => {
                    self.app_core.apply_macro_delete(group, label);
                }
                crate::core::remote::RemoteEvent::Notice(message) => {
                    self.app_core.add_system_message(&message);
                }
                crate::core::remote::RemoteEvent::LauncherSshGet {
                    client_id,
                    request_id,
                } => {
                    self.app_core
                        .handle_remote_launcher_ssh_get(client_id, request_id);
                }
                crate::core::remote::RemoteEvent::LauncherSshPut {
                    client_id,
                    request_id,
                    user,
                    host,
                    port,
                    remote_os,
                    generate_key,
                } => {
                    self.app_core.handle_remote_launcher_ssh_put(
                        client_id,
                        request_id,
                        user,
                        host,
                        port,
                        remote_os,
                        generate_key,
                    );
                }
                crate::core::remote::RemoteEvent::ConfigGet {
                    client_id,
                    request_id,
                    file,
                } => {
                    self.app_core
                        .handle_remote_config_get(client_id, request_id, file);
                }
                crate::core::remote::RemoteEvent::ConfigPut {
                    client_id,
                    request_id,
                    file,
                    content,
                } => {
                    self.app_core
                        .handle_remote_config_put(client_id, request_id, file, content);
                }
                crate::core::remote::RemoteEvent::HighlightsGet {
                    client_id,
                    request_id,
                    scope,
                } => {
                    self.app_core
                        .handle_remote_highlights_get(client_id, request_id, scope);
                }
                crate::core::remote::RemoteEvent::HighlightPut {
                    client_id,
                    request_id,
                    scope,
                    name,
                    rule,
                } => {
                    self.app_core
                        .handle_remote_highlight_put(client_id, request_id, scope, name, rule);
                }
                crate::core::remote::RemoteEvent::SettingsGet {
                    client_id,
                    request_id,
                } => {
                    self.app_core
                        .handle_remote_settings_get(client_id, request_id);
                }
                crate::core::remote::RemoteEvent::SettingsPut {
                    client_id,
                    request_id,
                    key,
                    value,
                    scope,
                    clear,
                } => {
                    self.app_core.handle_remote_settings_put(
                        client_id, request_id, key, value, scope, clear,
                    );
                }
                crate::core::remote::RemoteEvent::StreamsGet {
                    client_id,
                    request_id,
                } => {
                    self.app_core
                        .handle_remote_streams_get(client_id, request_id);
                }
                crate::core::remote::RemoteEvent::StreamsPut {
                    client_id,
                    request_id,
                    stream,
                    target,
                } => {
                    self.app_core
                        .handle_remote_streams_put(client_id, request_id, stream, target);
                }
                crate::core::remote::RemoteEvent::ColorsGet {
                    client_id,
                    request_id,
                    scope,
                } => {
                    self.app_core
                        .handle_remote_colors_get(client_id, request_id, scope);
                }
                crate::core::remote::RemoteEvent::ColorsPut {
                    client_id,
                    request_id,
                    scope,
                    colors,
                } => {
                    self.app_core
                        .handle_remote_colors_put(client_id, request_id, scope, colors);
                }
                crate::core::remote::RemoteEvent::TouchWheelGet {
                    client_id,
                    request_id,
                    scope,
                } => {
                    self.app_core
                        .handle_remote_touch_wheel_get(client_id, request_id, scope);
                }
                crate::core::remote::RemoteEvent::TouchWheelPut {
                    client_id,
                    request_id,
                    scope,
                    slices,
                } => {
                    self.app_core
                        .handle_remote_touch_wheel_put(client_id, request_id, scope, slices);
                }
                crate::core::remote::RemoteEvent::WebUiSubscribe { page } => {
                    self.app_core.webui_subscribe(&page);
                }
                crate::core::remote::RemoteEvent::WebUiUnsubscribe { page } => {
                    self.app_core.webui_unsubscribe(&page);
                }
                crate::core::remote::RemoteEvent::WebUiEvent { page, cid, value } => {
                    self.app_core.webui_send_event(page, cid, value);
                }
                crate::core::remote::RemoteEvent::MapLocations {
                    client_id,
                    request_id,
                } => {
                    self.app_core
                        .handle_remote_map_locations(client_id, request_id);
                }
                crate::core::remote::RemoteEvent::MapView {
                    client_id,
                    request_id,
                    location,
                } => {
                    self.app_core
                        .handle_remote_map_view(client_id, request_id, location);
                }
                crate::core::remote::RemoteEvent::HighlightDelete {
                    client_id,
                    request_id,
                    scope,
                    name,
                } => {
                    self.app_core
                        .handle_remote_highlight_delete(client_id, request_id, scope, name);
                }
                crate::core::remote::RemoteEvent::SessionConnect { .. }
                | crate::core::remote::RemoteEvent::SessionDisconnect
                | crate::core::remote::RemoteEvent::SessionStop
                | crate::core::remote::RemoteEvent::SessionExitLogout => {
                    // Sidecar sessions are owned by this local UI; the web
                    // client shouldn't offer these (session_control is
                    // false), but answer stray requests politely.
                    self.app_core
                        .add_system_message("Session control is only available in headless mode.");
                }
                crate::core::remote::RemoteEvent::Macro { id } => {
                    // Resolve the id against config; the command runs the
                    // same dispatch as typed input (echo, dot-commands).
                    match self.app_core.config.macros.resolve(&id).map(String::from) {
                        Some(command) => {
                            tracing::debug!("remote macro '{}': '{}'", id, command);
                            self.dispatch_command(command);
                        }
                        None => tracing::warn!(
                            "remote macro id '{}' did not resolve (stale client?)",
                            id
                        ),
                    }
                }
                crate::core::remote::RemoteEvent::WheelPick { key, path } => {
                    // Resolved against config (or the dynamic portals
                    // wheel) like macros; same dispatch as typed input.
                    match self.app_core.wheel_pick_command(&key, &path) {
                        Some(command) => {
                            tracing::debug!(
                                "remote wheel pick '{}' {:?}: '{}'",
                                key,
                                path,
                                command
                            );
                            self.dispatch_command(command);
                        }
                        None => tracing::warn!(
                            "remote wheel pick '{}' {:?} did not resolve (stale client?)",
                            key,
                            path
                        ),
                    }
                }
                crate::core::remote::RemoteEvent::SkillTrainerOpen => {
                    // Re-mirror if a page is loaded, else fetch a fresh one.
                    if self.app_core.ui_state.skill_trainer.data.is_some() {
                        self.app_core.ui_state.skill_trainer.open = true;
                    } else {
                        let cmd = self.app_core.skill_trainer_reload_command();
                        self.dispatch_command(cmd);
                    }
                    self.app_core.push_skill_trainer_remote();
                }
                crate::core::remote::RemoteEvent::SkillTrainerReload => {
                    // `goals` dispatches through the same path as typed input.
                    let cmd = self.app_core.skill_trainer_reload_command();
                    self.dispatch_command(cmd);
                    self.app_core.push_skill_trainer_remote();
                }
                crate::core::remote::RemoteEvent::SkillTrainerStep { id, n, raise } => {
                    self.app_core.skill_trainer_step(id, n, raise);
                    self.app_core.push_skill_trainer_remote();
                }
                crate::core::remote::RemoteEvent::SkillTrainerApply => {
                    self.app_core.skill_trainer_apply();
                    self.app_core.push_skill_trainer_remote();
                }
                crate::core::remote::RemoteEvent::SkillTrainerProfileSave { name } => {
                    self.app_core.skill_trainer_save_profile(&name);
                    self.app_core.invalidate_skill_trainer_remote();
                    self.app_core.push_skill_trainer_remote();
                }
                crate::core::remote::RemoteEvent::SkillTrainerProfileLoad { name } => {
                    self.app_core.skill_trainer_load_profile(&name);
                    self.app_core.push_skill_trainer_remote();
                }
                crate::core::remote::RemoteEvent::SkillTrainerProfileDelete { name } => {
                    self.app_core.skill_trainer_delete_profile(&name);
                    self.app_core.invalidate_skill_trainer_remote();
                    self.app_core.push_skill_trainer_remote();
                }
            }
        }

        // Drain map worker results (mapdb load, layout generation), the
        // mapdb release updater, and the walk executor.
        self.app_core.poll_map();
        // Commands the walk executor queued go out through the same path as
        // typed commands (echo, ghost-room labels, network).
        for command in self.app_core.take_outbound() {
            self.dispatch_command(command);
        }

        let mut received_text = false;
        // Backlog before this drain = how far behind the UI is on server
        // messages (the GUI's event queue).
        self.app_core
            .perf_stats
            .record_event_queue_depth(self.server_rx.len() as u64);
        while let Ok(message) = self.server_rx.try_recv() {
            let event_start = std::time::Instant::now();
            match message {
                ServerMessage::Text(line) => {
                    // First data from the game = connection established:
                    // time the login music from here.
                    if self.startup_music_pending {
                        self.startup_music_pending = false;
                        self.startup_music_at = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_millis(
                                    self.app_core.config.sound.startup_music_delay_ms,
                                ),
                        );
                    }
                    self.app_core
                        .perf_stats
                        .record_bytes_received((line.len() + 1) as u64);
                    if let Err(err) = self.app_core.process_server_data(&line) {
                        self.app_core
                            .add_system_message(&format!("GUI parse error: {}", err));
                    }
                    self.app_core.needs_render = true;
                    received_text = true;
                }
                ServerMessage::Connected => {
                    self.app_core.game_state.connected = true;
                    self.app_core.needs_render = true;
                    // Layout has saved WebUI panels: bring them back up
                    // automatically (Lich proxy connections only - a direct
                    // connection has no Lich to answer the handshake).
                    if !self.is_direct_connection
                        && !self.webui_handshake_sent
                        && self.has_webui_windows()
                    {
                        self.request_webui_handshake();
                    }
                }
                ServerMessage::Disconnected => {
                    self.app_core.game_state.connected = false;
                    self.app_core.clear_pending_goals_launches();
                    self.app_core.needs_render = true;
                }
            }
            self.app_core
                .perf_stats
                .record_event_process_time(event_start.elapsed());
        }

        // Post-processing the TUI runtime also performs after server data:
        // content-driven resizes, plus realizing game-offered windows
        // (containers whose offer the user has Shown, openDialog-templated
        // widgets like stance/inventory/experience).
        if received_text {
            // Room facts (nav rm, title, subtitle) may have landed in this
            // drain, but the scene tick in poll_map ran BEFORE it — re-pick
            // now so a scene swap renders in the same frame as the room
            // text instead of trailing it by a frame (a visible "lookup"
            // beat). Compare-only when nothing changed.
            self.app_core.tick_stage_scene();
            self.app_core.adjust_content_driven_windows();
            let (layout_width, layout_height) = self.core_layout_size;
            self.app_core
                .realize_offered_windows(layout_width, layout_height);

            // A `;ui handshake` reply arrived on the game stream: connect
            // (or reconnect) the WebUI bridge with the fresh port + token.
            if let Some(handshake) = self
                .app_core
                .message_processor
                .pending_webui_handshake
                .take()
            {
                self.handle_webui_handshake(handshake);
            }
        }

        // Core owns the WebUI socket: drain it (fans events to the phone and
        // re-emits to the GUI channel), then the GUI applies them to panels.
        self.app_core.pump_webui();
        self.pump_webui_events();

        // Drain SSH-launcher progress from any in-flight `.launch` flow; this
        // surfaces status and, on Ready, attaches to the new Lich session.
        self.pump_launch_progress();

        // Flush coalesced state deltas to web clients once per batch
        // (no-op unless [web] is enabled)
        self.app_core.flush_remote_state();

        // Send commands queued by dialog-panel widgets this frame
        // (they render from an immutable AppCore borrow).
        let panel_commands: Vec<String> = self
            .app_core
            .ui_state
            .pending_panel_commands
            .borrow_mut()
            .drain(..)
            .collect();
        for command in panel_commands {
            // Client-side panel verbs never reach the game. A dialog's
            // closeButton hides the window hosting it (bound layout window
            // by binding id, or the legacy ephemeral panel_<id>).
            if let Some(dialog_id) = command.strip_prefix("__VELLUM_CLOSE_PANEL__") {
                let name = self
                    .app_core
                    .layout
                    .windows
                    .iter()
                    .find(|w| {
                        w.base()
                            .binding
                            .as_ref()
                            .is_some_and(|b| b.id() == dialog_id)
                    })
                    .map(|w| w.name().to_string())
                    .unwrap_or_else(|| {
                        format!("panel_{}", dialog_id.replace(' ', "_").to_lowercase())
                    });
                let (w, h) = self.core_layout_size;
                self.app_core.set_known_window_shown(&name, false, w, h);
                continue;
            }
            self.dispatch_raw_command(command);
        }

        // Play sounds queued by highlight processing.
        for sound in self.app_core.game_state.drain_sound_queue() {
            if let Some(ref player) = self.app_core.sound_player {
                if let Err(err) = player.play_from_sounds_dir(&sound.file, sound.volume) {
                    tracing::warn!("Failed to play sound '{}': {}", sound.file, err);
                }
            }
        }

        // Poll TTS callback events for auto-play.
        self.app_core.poll_tts_events();
    }
}
