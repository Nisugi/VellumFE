//! AppCore surface for the native skill trainer panel.
//!
//! Flow: the user sends `goals` (typed, or via `.goals`) → [`arm_skill_trainer`]
//! marks the next `<LaunchURL>` as trainer traffic → `poll_skill_trainer`
//! (called every frame from `poll_map`) hands that URL to the worker instead
//! of the browser, then drains fetch/submit results into
//! `ui_state.skill_trainer` for both frontends to render.

use crate::core::skill_trainer::{self, TrainerEvent};
use crate::data::skill_trainer::{GoalProfile, TrainerStatus};

use super::AppCore;

/// How long an armed GOALS request may wait for its LaunchURL before an
/// unrelated LaunchURL falls back to the browser. Generous: the game answers
/// in well under a second.
const ARM_WINDOW: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GoalsLaunchTarget {
    NativeTrainer,
    HostBrowser,
    RemoteBrowser(u64),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingGoalsLaunch {
    target: GoalsLaunchTarget,
    armed: std::time::Instant,
}

impl AppCore {
    /// Send a command originating from one remote browser connection.
    ///
    /// Most commands are identical to local input. The GOALS browser route is
    /// an addressed request/reply flow: retain its origin so the eventual game
    /// LaunchURL returns to that browser instead of opening a native panel or
    /// invoking the host operating system. Browser-originated `goals web`
    /// uses the same addressed route; locally entered `goals web`
    /// retains its existing host-browser behavior through `send_command`.
    pub fn send_remote_command(
        &mut self,
        client_id: u64,
        command: String,
    ) -> anyhow::Result<crate::data::CommandOutcome> {
        let command_text = command.trim();
        let plain_goals = command_text.eq_ignore_ascii_case("goals");
        let routes_browser = plain_goals
            || command_text.eq_ignore_ascii_case("goals web")
            || command_text.eq_ignore_ascii_case(".goals web");
        let previous_trainer_ui = plain_goals.then(|| self.ui_state.skill_trainer.clone());
        let result = self.send_command(command);
        if let Some(previous) = previous_trainer_ui {
            // send_command's local GOALS path opens the native trainer. A
            // browser-originated command must leave that presentation exactly
            // as it found it.
            self.ui_state.skill_trainer = previous;
        }
        if routes_browser && result.is_ok() {
            self.staged_goals_launch = Some(GoalsLaunchTarget::RemoteBrowser(client_id));
        } else if result.is_err() {
            self.staged_goals_launch = None;
        }
        result
    }

    /// Mark the next LaunchURL as skill-trainer traffic and show the panel
    /// in its loading state.
    pub fn arm_skill_trainer(&mut self) {
        self.staged_goals_launch = Some(GoalsLaunchTarget::NativeTrainer);
        let ui = &mut self.ui_state.skill_trainer;
        ui.open = true;
        ui.status = TrainerStatus::Loading;
    }

    /// Mark the next GOALS LaunchURL for the host's ordinary browser.
    pub(super) fn arm_goals_host_browser(&mut self) {
        self.staged_goals_launch = Some(GoalsLaunchTarget::HostBrowser);
    }

    /// Complete the lifecycle of one command handed to a frontend's network
    /// queue. Only a successfully queued GOALS command earns a reply target;
    /// failures discard the staged target so they cannot shift later replies.
    ///
    /// Internal trainer refreshes can hand the literal command directly to a
    /// network sender, bypassing `send_command`, so an otherwise unstaged
    /// successful GOALS send defaults to the native trainer.
    pub fn finish_game_command_send(&mut self, command: &str, sent: bool) {
        if !command.trim().eq_ignore_ascii_case("goals") {
            return;
        }
        let target = self
            .staged_goals_launch
            .take()
            .unwrap_or(GoalsLaunchTarget::NativeTrainer);
        if sent {
            self.pending_goals_launches.push_back(PendingGoalsLaunch {
                target,
                armed: std::time::Instant::now(),
            });
        }
    }

    /// Forget reply ownership from a transport that can no longer produce
    /// replies. A fresh game session must never spend its first LaunchURL on
    /// a tombstone inherited from the previous connection.
    pub fn clear_pending_goals_launches(&mut self) {
        self.staged_goals_launch = None;
        self.pending_goals_launches.clear();
        self.message_processor.pending_launch_urls.clear();
    }

    /// Per-frame: route a pending LaunchURL and drain worker results.
    pub fn poll_skill_trainer(&mut self) {
        while let Some(url) = self.message_processor.pending_launch_urls.pop_front() {
            let pending = self.pending_goals_launches.pop_front();
            let expired = pending.is_some_and(|pending| pending.armed.elapsed() >= ARM_WINDOW);
            let target = pending.filter(|_| !expired).map(|pending| pending.target);
            if expired {
                // The protocol has no request id. Preserve FIFO position by
                // consuming this tombstone with exactly one URL; assigning
                // the URL to the next live target would cross-route a late
                // one-time ticket. Fail closed instead.
                tracing::warn!("Discarding LaunchURL for expired GOALS request");
            } else if let Some(GoalsLaunchTarget::RemoteBrowser(client_id)) = target {
                let full_url = format!("https://www.play.net{url}");
                if let Some(remote) = self.message_processor.remote.as_mut() {
                    remote.push_open_url(client_id, full_url);
                }
            } else if target == Some(GoalsLaunchTarget::NativeTrainer) {
                self.add_system_message("[goals] fetching the skill manager page...");
                self.skill_trainer_worker.fetch(&url);
            } else {
                let full_url = format!("https://www.play.net{url}");
                tracing::info!("Launching URL in browser: {}", full_url);
                if let Err(e) = crate::platform::open_url(&full_url) {
                    tracing::error!("Failed to open browser: {}", e);
                }
            }
        }

        for event in self.skill_trainer_worker.poll() {
            let ui = &mut self.ui_state.skill_trainer;
            match event {
                TrainerEvent::Loaded(goals) => {
                    ui.data = Some(*goals);
                    ui.status = TrainerStatus::Idle;
                    ui.revision += 1;
                    ui.open = true;
                    self.add_system_message("[goals] skill manager loaded.");
                }
                TrainerEvent::Applied(goals) => {
                    ui.data = Some(*goals);
                    ui.status = TrainerStatus::Idle;
                    ui.revision += 1;
                    self.add_system_message("[goals] skill goals applied.");
                }
                TrainerEvent::Saved => {
                    // Goals committed and pushed to the game. The confirmation
                    // page isn't a trainer form, so re-fetch to refresh the
                    // panel with the now-committed ranks.
                    ui.status = TrainerStatus::Loading;
                    self.add_system_message(
                        "[goals] skill goals saved and sent to the game. Refreshing…",
                    );
                    let cmd = self.skill_trainer_reload_command();
                    // Small delay so play.net's write settles before the
                    // fresh GOALS link fetches the updated page.
                    self.queue_timed_command(std::time::Duration::from_millis(750), cmd);
                }
                TrainerEvent::Failed(msg) => {
                    ui.status = TrainerStatus::Error(msg.clone());
                    self.add_system_message(&format!("[goals] {msg}"));
                }
            }
        }

        // Mirror the panel to remote clients when anything observable changed.
        self.push_skill_trainer_remote();
    }

    /// Broadcast the skill-trainer panel to remote clients, but only when its
    /// observable state (open, status, data revision) changed since the last
    /// push. Guarded on `remote.is_some()` exactly like `flush_remote_state`.
    pub fn push_skill_trainer_remote(&mut self) {
        if self.message_processor.remote.is_none() {
            return;
        }
        let (open, status, revision) = {
            let ui = &self.ui_state.skill_trainer;
            (
                ui.open,
                Self::skill_trainer_status_tag(&ui.status),
                ui.revision,
            )
        };
        // The sink owns dedup (its `last_*` fingerprints already live there);
        // it only asks us to serialize when the fingerprint actually changed.
        let needs_push = self
            .message_processor
            .remote
            .as_ref()
            .is_some_and(|r| r.skill_trainer_changed(open, &status, revision));
        if !needs_push {
            return;
        }
        let data = self.skill_trainer_remote_json();
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_skill_trainer(open, status, revision, data);
        }
    }

    /// Force the next `push_skill_trainer_remote` to broadcast even if the
    /// (open, status, revision) fingerprint is unchanged — used when only the
    /// profile list changed (profile save/delete), which the fingerprint does
    /// not observe.
    pub fn invalidate_skill_trainer_remote(&mut self) {
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.invalidate_skill_trainer();
        }
    }

    /// A stable wire tag for the trainer status. `Error` carries its message
    /// as `error:<msg>` so the phone can show it.
    fn skill_trainer_status_tag(status: &TrainerStatus) -> String {
        match status {
            TrainerStatus::Idle => "idle".to_string(),
            TrainerStatus::Loading => "loading".to_string(),
            TrainerStatus::Applying => "applying".to_string(),
            TrainerStatus::Error(msg) => format!("error:{msg}"),
        }
    }

    /// Serialize the trainer panel into the JSON the phone renders directly:
    /// `{ rows: [{id,name,section,ranks,goal,max,phy_cost,mnt_cost}],
    ///    points: {phy_left,mnt_left,phy_conv,mnt_conv},
    ///    char: {name,level,prof}, dirty, profiles: [..] }`.
    /// None when no page has loaded yet (panel still empty).
    pub fn skill_trainer_remote_json(&self) -> Option<serde_json::Value> {
        let goals = self.ui_state.skill_trainer.data.as_ref()?;
        let rows: Vec<serde_json::Value> = goals
            .rows
            .iter()
            .map(|row| {
                let (phy_cost, mnt_cost) = goals.cost_to_raise(row.id);
                serde_json::json!({
                    "id": row.id,
                    "name": row.name,
                    "section": row.section,
                    "ranks": goals.start_ranks_of(row.id),
                    "goal": goals.goal_ranks(row.id),
                    "max": goals.max_ranks_of(row.id),
                    "phy_cost": phy_cost,
                    "mnt_cost": mnt_cost,
                })
            })
            .collect();
        Some(serde_json::json!({
            "rows": rows,
            "points": {
                "phy_left": goals.phy_left,
                "mnt_left": goals.mnt_left,
                "phy_conv": goals.phy_conv,
                "mnt_conv": goals.mnt_conv,
            },
            "char": {
                "name": goals.char_name,
                "level": goals.level,
                "prof": goals.prof_name,
            },
            "dirty": goals.dirty(),
            "profiles": self.skill_trainer_profiles(),
        }))
    }

    /// Step a skill's goal by `n` ranks (frontends' +/- buttons; n is the
    /// 1/10/100 step). Returns how many ranks actually applied.
    pub fn skill_trainer_step(&mut self, id: u32, n: u32, raise: bool) -> u32 {
        let applied = self
            .ui_state
            .skill_trainer
            .data
            .as_mut()
            .map(|g| g.step(id, n, raise))
            .unwrap_or(0);
        // Bump the revision so both frontend caches and the remote push
        // fingerprint observe the goal change (step mutates data in place).
        if applied > 0 {
            self.ui_state.skill_trainer.revision += 1;
        }
        applied
    }

    /// POST the current goals to play.net.
    pub fn skill_trainer_apply(&mut self) {
        let ui = &mut self.ui_state.skill_trainer;
        if ui.status == TrainerStatus::Applying {
            return;
        }
        let Some(goals) = ui.data.clone() else {
            return;
        };
        ui.status = TrainerStatus::Applying;
        self.skill_trainer_worker.submit(&goals);
        self.add_system_message("[goals] submitting skill goals...");
    }

    /// Re-request a fresh page: sends `goals` to the game and arms the
    /// trainer. Returns the command for the network layer.
    pub fn skill_trainer_reload_command(&mut self) -> String {
        // Presentation state changes now; the reply target is staged only
        // when the command actually reaches a frontend's send path. This is
        // important for the delayed post-save refresh.
        let ui = &mut self.ui_state.skill_trainer;
        ui.open = true;
        ui.status = TrainerStatus::Loading;
        "goals".to_string()
    }

    /// Saved goal profiles for the current character, sorted by name.
    pub fn skill_trainer_profiles(&self) -> Vec<String> {
        let who = self.skill_trainer_character();
        skill_trainer::load_profiles()
            .get(&who)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn skill_trainer_save_profile(&mut self, name: &str) {
        let who = self.skill_trainer_character();
        let Some(goals) = self.ui_state.skill_trainer.data.as_ref() else {
            return;
        };
        let profile = GoalProfile::capture(goals);
        let mut store = skill_trainer::load_profiles();
        store.entry(who).or_default().insert(name.to_string(), profile);
        match skill_trainer::save_profiles(&store) {
            Ok(()) => self.add_system_message(&format!("[goals] profile '{name}' saved.")),
            Err(e) => self.add_system_message(&format!("[goals] saving profile failed: {e}")),
        }
    }

    /// Load a named profile into the editor (goals only — Apply still
    /// required to commit on the server).
    pub fn skill_trainer_load_profile(&mut self, name: &str) {
        let who = self.skill_trainer_character();
        let store = skill_trainer::load_profiles();
        let Some(profile) = store.get(&who).and_then(|m| m.get(name)) else {
            self.add_system_message(&format!("[goals] no profile named '{name}'."));
            return;
        };
        let profile = profile.clone();
        if let Some(goals) = self.ui_state.skill_trainer.data.as_mut() {
            skill_trainer::apply_profile(goals, &profile);
            self.ui_state.skill_trainer.revision += 1;
            self.add_system_message(&format!("[goals] profile '{name}' loaded into the editor."));
        }
    }

    pub fn skill_trainer_delete_profile(&mut self, name: &str) {
        let who = self.skill_trainer_character();
        let mut store = skill_trainer::load_profiles();
        let removed = store
            .get_mut(&who)
            .map(|m| m.remove(name).is_some())
            .unwrap_or(false);
        if removed {
            match skill_trainer::save_profiles(&store) {
                Ok(()) => self.add_system_message(&format!("[goals] profile '{name}' deleted.")),
                Err(e) => self.add_system_message(&format!("[goals] deleting profile failed: {e}")),
            }
        } else {
            self.add_system_message(&format!("[goals] no profile named '{name}'."));
        }
    }

    /// Profile-store key: the trainer page's character name when loaded
    /// (authoritative), else the session character.
    fn skill_trainer_character(&self) -> String {
        self.ui_state
            .skill_trainer
            .data
            .as_ref()
            .map(|g| g.char_name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.game_state.character_name.clone().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::CommandOutcome;

    #[test]
    fn local_and_remote_goals_keep_distinct_presentation_routes() {
        let mut local = AppCore::new_for_test();
        let CommandOutcome::Game(local_command) =
            local.send_command("GOALS".to_string()).unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        assert!(local.pending_goals_launches.is_empty());
        local.finish_game_command_send(&local_command, true);
        assert_eq!(
            local
                .pending_goals_launches
                .front()
                .map(|pending| pending.target),
            Some(GoalsLaunchTarget::NativeTrainer)
        );
        assert!(local.ui_state.skill_trainer.open);

        let mut remote = AppCore::new_for_test();
        let (sink, handles, _event_rx) = crate::core::remote::RemoteSink::new(8);
        let mut delta_rx = handles.delta_tx.subscribe();
        remote.enable_remote(sink);
        let CommandOutcome::Game(remote_command) = remote
            .send_remote_command(73, "GOALS".to_string())
            .unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        assert!(remote.pending_goals_launches.is_empty());
        remote.finish_game_command_send(&remote_command, true);
        assert_eq!(
            remote
                .pending_goals_launches
                .front()
                .map(|pending| pending.target),
            Some(GoalsLaunchTarget::RemoteBrowser(73))
        );
        assert!(!remote.ui_state.skill_trainer.open);

        remote
            .message_processor
            .pending_launch_urls
            .push_back("/gs4/play/cm/loader.asp?ticket=shared-seam".to_string());
        remote.poll_skill_trainer();
        let addressed = std::iter::from_fn(|| delta_rx.try_recv().ok()).find_map(|delta| {
            if let crate::core::remote::RemoteDelta::OpenUrl { client_id, url } = delta {
                Some((client_id, url))
            } else {
                None
            }
        });
        assert_eq!(
            addressed,
            Some((
                73,
                "https://www.play.net/gs4/play/cm/loader.asp?ticket=shared-seam".to_string()
            ))
        );
    }

    #[test]
    fn local_goals_web_retains_its_existing_external_route() {
        let mut core = AppCore::new_for_test();
        let CommandOutcome::Game(command) = core.send_command("GOALS web".to_string()).unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&command, true);
        assert_eq!(
            core.pending_goals_launches
                .front()
                .map(|pending| pending.target),
            Some(GoalsLaunchTarget::HostBrowser)
        );
        assert!(!core.ui_state.skill_trainer.open);
    }

    #[test]
    fn dotted_remote_goals_web_returns_to_the_requesting_browser() {
        let mut core = AppCore::new_for_test();
        let CommandOutcome::Game(command) = core
            .send_remote_command(73, ".GOALS web".to_string())
            .unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&command, true);
        assert_eq!(
            core.pending_goals_launches
                .front()
                .map(|pending| pending.target),
            Some(GoalsLaunchTarget::RemoteBrowser(73))
        );
    }

    #[test]
    fn overlapping_browser_goals_replies_preserve_request_order() {
        let mut core = AppCore::new_for_test();
        let (sink, handles, _event_rx) = crate::core::remote::RemoteSink::new(8);
        let mut delta_rx = handles.delta_tx.subscribe();
        core.enable_remote(sink);

        let CommandOutcome::Game(first) =
            core.send_remote_command(11, "GOALS".to_string()).unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&first, true);
        let CommandOutcome::Game(second) = core
            .send_remote_command(22, "GOALS web".to_string())
            .unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&second, true);

        // Frontends drain all available server messages as one batch. Both
        // replies therefore need to survive parsing until the next core poll.
        for ticket in ["first", "second"] {
            core.message_processor
                .pending_launch_urls
                .push_back(format!("/gs4/play/cm/loader.asp?ticket={ticket}"));
        }
        core.poll_skill_trainer();

        let addressed: Vec<_> = std::iter::from_fn(|| delta_rx.try_recv().ok())
            .filter_map(|delta| match delta {
                crate::core::remote::RemoteDelta::OpenUrl { client_id, url } => {
                    Some((client_id, url))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            addressed,
            vec![
                (
                    11,
                    "https://www.play.net/gs4/play/cm/loader.asp?ticket=first".to_string(),
                ),
                (
                    22,
                    "https://www.play.net/gs4/play/cm/loader.asp?ticket=second".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn trainer_reload_is_committed_once_at_actual_send_time() {
        let mut core = AppCore::new_for_test();

        let delayed = core.skill_trainer_reload_command();
        assert!(core.pending_goals_launches.is_empty());
        assert!(core.staged_goals_launch.is_none());

        let CommandOutcome::Game(remote) =
            core.send_remote_command(11, "GOALS".to_string()).unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&remote, true);

        let CommandOutcome::Game(native) = core.send_command(delayed).unwrap() else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&native, true);

        assert_eq!(
            core.pending_goals_launches
                .iter()
                .map(|pending| pending.target)
                .collect::<Vec<_>>(),
            vec![
                GoalsLaunchTarget::RemoteBrowser(11),
                GoalsLaunchTarget::NativeTrainer,
            ]
        );
    }

    #[test]
    fn failed_send_rolls_back_target_without_shifting_the_next_reply() {
        let mut core = AppCore::new_for_test();
        let (sink, handles, _event_rx) = crate::core::remote::RemoteSink::new(8);
        let mut delta_rx = handles.delta_tx.subscribe();
        core.enable_remote(sink);

        let CommandOutcome::Game(failed) =
            core.send_remote_command(11, "GOALS".to_string()).unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&failed, false);

        let CommandOutcome::Game(sent) =
            core.send_remote_command(22, "GOALS".to_string()).unwrap()
        else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&sent, true);
        core.message_processor
            .pending_launch_urls
            .push_back("/goals?ticket=second".to_string());
        core.poll_skill_trainer();

        assert!(std::iter::from_fn(|| delta_rx.try_recv().ok()).any(|delta| matches!(
            delta,
            crate::core::remote::RemoteDelta::OpenUrl { client_id: 22, .. }
        )));
    }

    #[test]
    fn expired_target_consumes_one_url_without_shifting_it_to_the_next_target() {
        let mut core = AppCore::new_for_test();
        let (sink, handles, _event_rx) = crate::core::remote::RemoteSink::new(8);
        let mut delta_rx = handles.delta_tx.subscribe();
        core.enable_remote(sink);

        for client_id in [11, 22] {
            let CommandOutcome::Game(command) = core
                .send_remote_command(client_id, "GOALS".to_string())
                .unwrap()
            else {
                panic!("GOALS must reach the game")
            };
            core.finish_game_command_send(&command, true);
        }
        core.pending_goals_launches.front_mut().unwrap().armed =
            std::time::Instant::now() - ARM_WINDOW;

        core.message_processor
            .pending_launch_urls
            .push_back("/goals?ticket=expired".to_string());
        core.poll_skill_trainer();
        assert!(std::iter::from_fn(|| delta_rx.try_recv().ok()).all(|delta| !matches!(
            delta,
            crate::core::remote::RemoteDelta::OpenUrl { .. }
        )));
        assert_eq!(
            core.pending_goals_launches
                .front()
                .map(|pending| pending.target),
            Some(GoalsLaunchTarget::RemoteBrowser(22))
        );

        core.message_processor
            .pending_launch_urls
            .push_back("/goals?ticket=second".to_string());
        core.poll_skill_trainer();
        assert!(std::iter::from_fn(|| delta_rx.try_recv().ok()).any(|delta| matches!(
            delta,
            crate::core::remote::RemoteDelta::OpenUrl { client_id: 22, .. }
        )));
    }

    #[test]
    fn disconnect_clears_committed_and_staged_targets() {
        let mut core = AppCore::new_for_test();
        let CommandOutcome::Game(first) = core.send_command("GOALS".to_string()).unwrap() else {
            panic!("GOALS must reach the game")
        };
        core.finish_game_command_send(&first, true);
        core.send_command("GOALS web".to_string()).unwrap();
        core.message_processor
            .pending_launch_urls
            .push_back("/stale".to_string());

        assert_eq!(core.pending_goals_launches.len(), 1);
        assert_eq!(
            core.staged_goals_launch,
            Some(GoalsLaunchTarget::HostBrowser)
        );

        core.clear_pending_goals_launches();
        assert!(core.pending_goals_launches.is_empty());
        assert!(core.staged_goals_launch.is_none());
        assert!(core.message_processor.pending_launch_urls.is_empty());
    }
}
