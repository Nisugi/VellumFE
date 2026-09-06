//! Graphical launcher - saved connection profiles and session spawning.
//!
//! Shown when vellum-fe starts with no arguments (double-click) or with
//! `--launcher`. Each profile row spawns a *separate process* running
//! `vellum-fe --launch-profile NAME` with a fingerprint of the confirmed
//! profile, so a session goes through the same startup path as a hand-typed
//! command line without a later profile edit retargeting it.
//!
//! Passwords never appear on a child's command line. A session resolves its
//! own password from the OS credential store; only when nothing is saved
//! does the launcher prompt and hand the password over via a private
//! environment variable (GUI sessions) or let the session prompt in its own
//! console (terminal sessions).

use anyhow::{anyhow, Context, Result};
use eframe::egui;
use std::process::Command;
use tokio::sync::mpsc;

use crate::config::profiles::{
    self, help, LaunchFrontend, LaunchMode, LaunchWebClient, LauncherProfile, LauncherStore,
    GAME_CHOICES,
};

/// Feedback line shown at the bottom of the launcher.
struct Status {
    text: String,
    is_error: bool,
}

impl Status {
    fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

/// Add/edit form state. `password` lives only in this struct and is dropped
/// with it; it is written to the OS credential store, never to disk.
struct EditForm {
    profile: LauncherProfile,
    /// Name before editing (None = new profile) so renames replace.
    original_name: Option<String>,
    original_account: String,
    original_password_saved: bool,
    password: String,
    /// Reveal the password field (eye toggle); resets with the form.
    show_password: bool,
    save_password: bool,
    web_enabled: bool,
    web_port_text: String,
    web_bind_text: String,
    port_text: String,
    /// Lich launch command, edited as plain text; empty commits as None.
    custom_launch_text: String,
    error: Option<String>,
}

impl EditForm {
    fn new_profile() -> Self {
        Self {
            profile: LauncherProfile::new_direct(),
            original_name: None,
            original_account: String::new(),
            original_password_saved: false,
            password: String::new(),
            show_password: false,
            save_password: true,
            web_enabled: false,
            web_port_text: "8484".to_string(),
            web_bind_text: "127.0.0.1".to_string(),
            port_text: "8000".to_string(),
            custom_launch_text: String::new(),
            error: None,
        }
    }

    fn edit(profile: LauncherProfile) -> Self {
        Self {
            original_name: Some(profile.name.clone()),
            original_account: profile.account.clone(),
            original_password_saved: profile.password_saved,
            password: String::new(),
            show_password: false,
            save_password: profile.password_saved,
            web_enabled: profile.web_port.is_some(),
            web_port_text: profile.web_port.unwrap_or(8484).to_string(),
            web_bind_text: profile
                .web_bind
                .clone()
                .unwrap_or_else(|| "127.0.0.1".to_string()),
            port_text: profile.port.to_string(),
            custom_launch_text: profile.custom_launch.clone().unwrap_or_default(),
            profile,
            error: None,
        }
    }
}

/// Modal shown when launching a GUI direct profile with no saved password.
struct PasswordPrompt {
    /// Immutable profile selected before any lifecycle work began.
    profile: LauncherProfile,
    launch_identity: crate::core::session_registry::SessionLaunchIdentity,
    password: String,
    show_password: bool,
    remember: bool,
    /// Held only when an old dormant runtime was stopped before this prompt.
    launch_claim: Option<crate::core::session_registry::LaunchClaim>,
}

impl PasswordPrompt {
    fn new(
        profile: LauncherProfile,
        launch_claim: Option<crate::core::session_registry::LaunchClaim>,
    ) -> Self {
        let launch_identity = crate::core::session_registry::SessionLaunchIdentity::from_profile(
            &profile.name,
            &profile,
        );
        Self {
            profile,
            launch_identity,
            password: String::new(),
            show_password: false,
            remember: true,
            launch_claim,
        }
    }

    fn verified_profile(
        &self,
        store: &LauncherStore,
    ) -> std::result::Result<LauncherProfile, String> {
        let Some(current) = store.find(&self.profile.name) else {
            return Err(format!(
                "The {} profile was deleted before launch; nothing was started.",
                self.profile.name
            ));
        };
        let current_identity = crate::core::session_registry::SessionLaunchIdentity::from_profile(
            &current.name,
            current,
        );
        let snapshot_identity = crate::core::session_registry::SessionLaunchIdentity::from_profile(
            &self.profile.name,
            &self.profile,
        );
        if current_identity != self.launch_identity || snapshot_identity != self.launch_identity {
            return Err(format!(
                "The {} profile changed while launch was pending; nothing was started.",
                self.profile.name
            ));
        }
        Ok(self.profile.clone())
    }
}

/// Confirmation state for handing a shared Lich endpoint to another
/// character.  Keeping the immutable request here ensures the modal cannot
/// accidentally confirm a different, later registry observation.
struct CharacterSwitchPrompt {
    request: crate::launcher::session_lifecycle::SwitchRequest,
    profile: LauncherProfile,
    dont_warn_again: bool,
}

type CharacterSwitchResult = (
    LauncherProfile,
    std::result::Result<crate::core::session_registry::LaunchClaim, String>,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharacterSwitchAction {
    RejectAttachOnly,
    RejectUncontrollableOwner,
    Confirm,
    Start,
}

fn character_switch_action(
    profile: &LauncherProfile,
    request: &crate::launcher::session_lifecycle::SwitchRequest,
    skip_warning: bool,
) -> CharacterSwitchAction {
    let has_launch_command = profile
        .custom_launch
        .as_deref()
        .is_some_and(|command| !command.trim().is_empty());
    if profile.mode != LaunchMode::Lich || !has_launch_command {
        CharacterSwitchAction::RejectAttachOnly
    } else if !request.owner_supports_character_handoff() {
        CharacterSwitchAction::RejectUncontrollableOwner
    } else if skip_warning {
        CharacterSwitchAction::Start
    } else {
        CharacterSwitchAction::Confirm
    }
}

pub struct LauncherApp {
    store: LauncherStore,
    edit: Option<EditForm>,
    password_prompt: Option<PasswordPrompt>,
    character_switch_prompt: Option<CharacterSwitchPrompt>,
    confirm_delete: Option<String>,
    status: Option<Status>,
    /// In-flight SSH launch (Lich profiles with a launch command): progress
    /// from the flow thread, plus the profile to spawn once the port is up.
    launch_progress_rx: Option<mpsc::UnboundedReceiver<crate::launcher::flow::LaunchProgress>>,
    pending_launch: Option<LauncherProfile>,
    pending_launch_claim: Option<crate::core::session_registry::LaunchClaim>,
    /// A confirmed character handoff owns its launch claim on this worker
    /// until the old runtime and Lich endpoint have both gone away.
    character_switch_rx: Option<mpsc::UnboundedReceiver<CharacterSwitchResult>>,
    /// A matching idle/disconnected runtime must stop before its replacement
    /// enters the ordinary launch flow.
    dormant_restart_rx: Option<mpsc::UnboundedReceiver<CharacterSwitchResult>>,
    /// Blocking HTTP stop requests run on a worker; the egui thread only
    /// polls this result channel.
    stop_progress_rx: Option<mpsc::UnboundedReceiver<(String, Result<(), String>)>>,
    /// Registry discovery probes the process table, so keep it out of the
    /// launcher's normal per-frame paint work. Launch clicks still perform an
    /// authoritative fresh read before deciding whether to spawn or resume.
    live_sessions: Vec<crate::core::session_registry::SessionEntry>,
    live_sessions_refreshed_at: std::time::Instant,
    /// Keep launcher-owned children until `try_wait` reaps them. Dropping a
    /// live `Child` handle leaves a zombie behind on Unix when that process
    /// later exits while the launcher remains open.
    session_children: Vec<std::process::Child>,
    /// Captured while the launcher executable still exists at this path. A
    /// development rebuild may replace that file while this process remains
    /// alive; resolving `/proc/self/exe` later would then point at `(deleted)`.
    session_executable: std::result::Result<std::path::PathBuf, String>,
}

impl LauncherApp {
    fn new() -> Self {
        let session_executable = std::env::current_exe()
            .map_err(|error| format!("Could not locate the vellum-fe executable: {error}"));
        let (store, status) = match LauncherStore::load() {
            Ok(store) => (store, None),
            Err(err) => (
                LauncherStore::default(),
                // Surface the parse error instead of silently starting
                // empty: saving from an empty list would overwrite the file.
                Some(Status::error(format!(
                    "Could not read launcher.toml ({err:#}). Fix or remove it - saving here will overwrite it."
                ))),
            ),
        };
        let live_sessions = crate::core::session_registry::list_and_gc();
        Self {
            store,
            edit: None,
            password_prompt: None,
            character_switch_prompt: None,
            confirm_delete: None,
            status,
            launch_progress_rx: None,
            pending_launch: None,
            pending_launch_claim: None,
            character_switch_rx: None,
            dormant_restart_rx: None,
            stop_progress_rx: None,
            live_sessions,
            live_sessions_refreshed_at: std::time::Instant::now(),
            session_children: Vec::new(),
            session_executable,
        }
    }

    fn refresh_live_sessions_if_stale(&mut self) {
        if self.live_sessions_refreshed_at.elapsed() < std::time::Duration::from_secs(2) {
            return;
        }
        self.live_sessions = crate::core::session_registry::list_and_gc();
        self.live_sessions_refreshed_at = std::time::Instant::now();
    }

    fn save_store(&mut self) {
        if let Err(err) = self.store.save() {
            self.status = Some(Status::error(format!("Failed to save profiles: {err:#}")));
        }
    }

    // ----- launching -------------------------------------------------------

    fn launch(&mut self, name: &str) {
        if self.launch_or_switch_busy() {
            self.status = Some(Status::error(
                "A launch or character switch is already in progress; wait for it to finish.",
            ));
            return;
        }
        let Some(profile) = self.store.find(name).cloned() else {
            return;
        };
        if self.resume_or_reject(&profile) {
            return;
        }
        match profile.mode {
            LaunchMode::Direct => {
                let saved = if profile.password_saved {
                    profiles::load_password(&profile.account)
                } else {
                    None
                };
                match saved {
                    // Session re-reads the credential store itself.
                    Some(_) => self.spawn(&profile, None),
                    // Nothing saved: collect the password here (with the
                    // option to remember it) and hand it over privately.
                    // This covers terminal sessions too - prompting in the
                    // launcher beats a bare console prompt with no way to
                    // save the password.
                    None => {
                        self.password_prompt = Some(PasswordPrompt::new(profile, None));
                    }
                }
            }
            LaunchMode::Lich => match profile.custom_launch.as_deref() {
                // Attach-only: Lich is expected to be up already.
                None => self.spawn(&profile, None),
                // Launch-capable: probe the port, SSH-start Lich if it's
                // down, wait for it, then spawn the session. The probe lives
                // inside the flow, so an already-running Lich costs one
                // short connect attempt before AlreadyRunning fires.
                Some(command) => self.start_ssh_launch(&profile, command),
            },
        }
    }

    fn launch_or_switch_busy(&self) -> bool {
        self.launch_progress_rx.is_some()
            || self.character_switch_rx.is_some()
            || self.dormant_restart_rx.is_some()
            || self.stop_progress_rx.is_some()
            || self.character_switch_prompt.is_some()
            || self.password_prompt.is_some()
    }

    /// Kick off the probe → SSH → wait-for-port flow on its own thread and
    /// remember which profile to spawn when it reports Ready.
    fn start_ssh_launch(&mut self, profile: &LauncherProfile, command: &str) {
        if self.launch_or_switch_busy() {
            self.status = Some(Status::error(
                "A launch or character switch is already in progress; wait for it to finish.",
            ));
            return;
        }
        let Some(claim) = self.reserve_launch(profile) else {
            return;
        };
        self.start_ssh_launch_reserved(profile, command, claim);
    }

    /// Enter the normal Lich probe/start flow with a claim acquired by the
    /// character-handoff worker.  This path must not reserve the endpoint a
    /// second time: the handoff claim closes the race between old and new
    /// runtime ownership.
    fn start_ssh_launch_reserved(
        &mut self,
        profile: &LauncherProfile,
        command: &str,
        claim: crate::core::session_registry::LaunchClaim,
    ) {
        if self.launch_progress_rx.is_some()
            || self.character_switch_rx.is_some()
            || self.dormant_restart_rx.is_some()
            || self.stop_progress_rx.is_some()
        {
            self.status = Some(Status::error(
                "A launch or character switch is already in progress; the target was not launched.",
            ));
            return;
        }
        // SSH settings are only REQUIRED for a remote launch. A same-machine
        // profile (loopback host) spawns directly, so a missing or unreadable
        // ssh-launcher.toml must not block it — that file is exactly what a
        // single-PC user has never created.
        let local = crate::launcher::flow::is_local_host(&profile.host);
        let ssh = match crate::launcher::config::LauncherConfig::load() {
            Ok(config) => config.ssh,
            Err(_) if local => Default::default(),
            Err(err) => {
                self.status = Some(Status::error(format!(
                    "Could not read the SSH launcher settings: {err:#}. Configure them with .launcher."
                )));
                return;
            }
        };
        let spec = crate::launcher::flow::LaunchSpec::from_command(
            command,
            &profile.host,
            profile.port,
            &profile.character,
            &ssh,
        );
        let (tx, rx) = mpsc::unbounded_channel();
        // Same constraint as the in-session launcher: russh's Handle isn't
        // provably Send across awaits, so this needs its own current-thread
        // runtime rather than the shared pool.
        let worker = std::thread::Builder::new()
            .name("ssh-launcher".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(err) => {
                        let _ = tx.send(crate::launcher::flow::LaunchProgress::Failed {
                            reason: format!("Could not start launcher runtime: {err}"),
                        });
                        return;
                    }
                };
                rt.block_on(async move {
                    let tx_progress = tx.clone();
                    let result = crate::launcher::flow::launch_spec(
                        &spec,
                        crate::launcher::flow::HostKeyTrust::AutoPinFirstUse,
                        |p| {
                            let _ = tx_progress.send(p);
                        },
                    )
                    .await;
                    // `launch_spec` reports failures through progress only in
                    // some paths; make sure a hard error always lands.
                    if let Err(err) = result {
                        let _ = tx.send(crate::launcher::flow::LaunchProgress::Failed {
                            reason: format!("{err:#}"),
                        });
                    }
                });
            });
        match worker {
            Ok(_) => {
                self.launch_progress_rx = Some(rx);
                self.pending_launch = Some(profile.clone());
                self.pending_launch_claim = Some(claim);
                self.status = Some(Status::ok(format!("Starting Lich for {}…", profile.name)));
            }
            Err(error) => {
                self.status = Some(Status::error(format!(
                    "Could not start the Lich launch worker: {error}; the target was not launched."
                )));
            }
        }
    }

    /// Drain SSH-launch progress into the status line; spawn the session
    /// once the port is up. Called once per frame.
    fn pump_launch_progress(&mut self, ctx: &egui::Context) {
        use crate::launcher::flow::LaunchProgress as LP;
        let mut ready = false;
        let mut finished = false;
        if let Some(rx) = self.launch_progress_rx.as_mut() {
            while let Ok(progress) = rx.try_recv() {
                match progress {
                    LP::Resolving { character } => {
                        self.status = Some(Status::ok(format!("Resolving {character}…")))
                    }
                    LP::AlreadyRunning { host, port } => {
                        self.status = Some(Status::ok(format!(
                            "Lich already running at {host}:{port} — attaching."
                        )))
                    }
                    LP::Connecting { host, port } => {
                        self.status =
                            Some(Status::ok(format!("Connecting to {host}:{port} over SSH…")))
                    }
                    LP::HostKeyPrompt { fingerprint } => {
                        self.status = Some(Status::ok(format!(
                            "Pinned new host key {fingerprint} (first use)."
                        )))
                    }
                    LP::HostKeyChanged => {
                        self.status = Some(Status::error(
                            "Host key CHANGED — refusing to connect (possible MITM).".to_string(),
                        ));
                        finished = true;
                    }
                    LP::Spawning { character } => {
                        self.status = Some(Status::ok(format!("Starting Lich for {character}…")))
                    }
                    LP::WaitingForPort { host, port } => {
                        self.status =
                            Some(Status::ok(format!("Waiting for Lich on {host}:{port}…")))
                    }
                    LP::Ready { .. } => {
                        ready = true;
                        finished = true;
                    }
                    LP::Failed { reason } => {
                        self.status = Some(Status::error(format!("Launch failed: {reason}")));
                        finished = true;
                    }
                }
            }
        }
        if ready {
            if let Some(profile) = self.pending_launch.clone() {
                let claim = self.pending_launch_claim.take();
                self.spawn_reserved(&profile, None, claim);
            }
        }
        if finished {
            self.launch_progress_rx = None;
            self.pending_launch = None;
            self.pending_launch_claim = None;
        }
        // A launch in flight produces progress from another thread; keep the
        // UI repainting so the status line actually advances.
        if self.launch_progress_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn spawn(&mut self, profile: &LauncherProfile, password: Option<&str>) {
        if self.launch_or_switch_busy() {
            self.status = Some(Status::error(
                "A launch or character switch is already in progress; wait for it to finish.",
            ));
            return;
        }
        if self.resume_or_reject(profile) {
            return;
        }
        let Some(claim) = self.reserve_launch(profile) else {
            return;
        };
        self.spawn_reserved(profile, password, Some(claim));
    }

    fn reserve_launch(
        &mut self,
        profile: &LauncherProfile,
    ) -> Option<crate::core::session_registry::LaunchClaim> {
        let identity = crate::core::session_registry::SessionLaunchIdentity::from_profile(
            &profile.name,
            profile,
        );
        match crate::core::session_registry::acquire_launch_claim(&identity) {
            Ok(crate::core::session_registry::LaunchClaimResult::Acquired(claim)) => Some(claim),
            Ok(crate::core::session_registry::LaunchClaimResult::Existing {
                profile,
                character,
            }) => {
                self.status = Some(Status::error(format!(
                    "A launch is already in progress for {character} ({profile})."
                )));
                None
            }
            Err(error) => {
                self.status = Some(Status::error(format!(
                    "Could not reserve this connection: {error:#}"
                )));
                None
            }
        }
    }

    fn spawn_reserved(
        &mut self,
        profile: &LauncherProfile,
        password: Option<&str>,
        claim: Option<crate::core::session_registry::LaunchClaim>,
    ) {
        let executable = match &self.session_executable {
            Ok(path) => path,
            Err(error) => {
                self.status = Some(Status::error(format!(
                    "Failed to launch {}: {error}",
                    profile.name
                )));
                return;
            }
        };
        match spawn_session(executable, profile, password) {
            Ok(child) => {
                self.session_children.push(child);
                if let Some(claim) = claim {
                    claim.persist_until_registration();
                }
                self.status = Some(Status::ok(format!("Launched {}", profile.name)));
            }
            Err(err) => {
                self.status = Some(Status::error(format!(
                    "Failed to launch {}: {err:#}",
                    profile.name
                )))
            }
        }
    }

    /// Return true when launching was handled by resuming, replacing a
    /// dormant runtime, or surfacing a conflict. Only `Spawn` falls through.
    fn resume_or_reject(&mut self, profile: &LauncherProfile) -> bool {
        use crate::launcher::session_lifecycle::{decide_launch, resume_url, LaunchDisposition};

        let requested = crate::core::session_registry::SessionLaunchIdentity::from_profile(
            &profile.name,
            profile,
        );
        match decide_launch(&requested, &crate::core::session_registry::list_and_gc()) {
            LaunchDisposition::Spawn => false,
            LaunchDisposition::Resume(entry) => {
                if let Some(client) = profile.web_client {
                    match resume_url(&entry, client).and_then(|url| crate::platform::open_url(&url))
                    {
                        Ok(()) => {
                            self.status = Some(Status::ok(format!(
                                "Reopened the existing {} session.",
                                profile.name
                            )))
                        }
                        Err(error) => {
                            self.status = Some(Status::error(format!(
                                "Session is already running, but could not reopen it: {error:#}"
                            )))
                        }
                    }
                } else {
                    self.status = Some(Status::ok(format!(
                        "{} is already running (process {}).",
                        profile.name, entry.pid
                    )));
                }
                true
            }
            LaunchDisposition::Replace(entry) => {
                self.start_dormant_restart(profile.clone(), entry, requested);
                true
            }
            LaunchDisposition::EndpointConflict(request) => {
                match character_switch_action(
                    profile,
                    &request,
                    self.store.skip_lich_switch_warning,
                ) {
                    CharacterSwitchAction::RejectAttachOnly => {
                        self.status = Some(Status::error(format!(
                            "Cannot launch {} ({}): Lich endpoint {}:{} is already owned by {} ({}). This attach-only profile cannot safely switch characters; stop the current session normally or add a Lich launch command.",
                            request.target_character(),
                            request.target_profile(),
                            request.host(),
                            request.port(),
                            request.current_character(),
                            request.current_profile()
                        )));
                    }
                    CharacterSwitchAction::RejectUncontrollableOwner => {
                        self.status = Some(Status::error(format!(
                            "Cannot switch {} to {} on Lich endpoint {}:{} because the current owner does not support launcher lifecycle control. Log out or close that native GUI/TUI session normally first.",
                            request.current_character(),
                            request.target_character(),
                            request.host(),
                            request.port(),
                        )));
                    }
                    CharacterSwitchAction::Confirm => {
                        self.character_switch_prompt = Some(CharacterSwitchPrompt {
                            request,
                            profile: profile.clone(),
                            dont_warn_again: false,
                        });
                    }
                    CharacterSwitchAction::Start => {
                        self.start_character_switch(profile.clone(), request);
                    }
                }
                true
            }
        }
    }

    fn start_dormant_restart(
        &mut self,
        profile: LauncherProfile,
        current: crate::core::session_registry::SessionEntry,
        target: crate::core::session_registry::SessionLaunchIdentity,
    ) {
        if self.launch_progress_rx.is_some()
            || self.character_switch_rx.is_some()
            || self.dormant_restart_rx.is_some()
            || self.stop_progress_rx.is_some()
        {
            self.status = Some(Status::error(
                "A launch or session restart is already in progress; wait for it to finish.",
            ));
            return;
        }

        let character = profile.character.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = std::thread::Builder::new()
            .name("dormant-session-restart".into())
            .spawn(move || {
                let result =
                    crate::launcher::session_lifecycle::replace_dormant_session(&current, &target)
                        .map_err(|error| format!("{error:#}"));
                let _ = tx.send((profile, result));
            });
        match worker {
            Ok(_) => {
                self.dormant_restart_rx = Some(rx);
                self.status = Some(Status::ok(format!(
                    "Stopping the dormant {character} session before relaunch…"
                )));
            }
            Err(error) => {
                self.status = Some(Status::error(format!(
                    "Could not start the session-restart worker: {error}; {character} was not launched."
                )));
            }
        }
    }

    fn finish_reserved_launch(
        &mut self,
        profile: LauncherProfile,
        claim: crate::core::session_registry::LaunchClaim,
    ) {
        match profile.mode {
            LaunchMode::Lich => match profile.custom_launch.clone() {
                Some(command) => self.start_ssh_launch_reserved(&profile, &command, claim),
                None => self.spawn_reserved(&profile, None, Some(claim)),
            },
            LaunchMode::Direct => {
                if profile.password_saved && profiles::load_password(&profile.account).is_some() {
                    self.spawn_reserved(&profile, None, Some(claim));
                } else {
                    self.password_prompt = Some(PasswordPrompt::new(profile, Some(claim)));
                }
            }
        }
    }

    fn pump_dormant_restart(&mut self, ctx: &egui::Context) {
        let result = self.dormant_restart_rx.as_mut().map(|rx| rx.try_recv());
        match result {
            Some(Ok((profile, Ok(claim)))) => {
                self.dormant_restart_rx = None;
                self.finish_reserved_launch(profile, claim);
            }
            Some(Ok((profile, Err(error)))) => {
                self.dormant_restart_rx = None;
                self.status = Some(Status::error(format!(
                    "Could not restart {}: {error}. The replacement was not launched.",
                    profile.character
                )));
            }
            Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                self.dormant_restart_rx = None;
                self.status = Some(Status::error(
                    "The session-restart worker ended without a result; the replacement was not launched.",
                ));
            }
            Some(Err(mpsc::error::TryRecvError::Empty)) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            None => {}
        }
    }

    fn start_character_switch(
        &mut self,
        profile: LauncherProfile,
        request: crate::launcher::session_lifecycle::SwitchRequest,
    ) {
        if self.launch_progress_rx.is_some()
            || self.character_switch_rx.is_some()
            || self.dormant_restart_rx.is_some()
            || self.stop_progress_rx.is_some()
        {
            self.status = Some(Status::error(
                "A launch or character switch is already in progress; wait for it to finish.",
            ));
            return;
        }

        let current = request.current_character().to_string();
        let target = request.target_character().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = std::thread::Builder::new()
            .name("character-switch".into())
            .spawn(move || {
                let result = crate::launcher::session_lifecycle::handoff_character_switch(&request)
                    .map_err(|error| format!("{error:#}"));
                let _ = tx.send((profile, result));
            });
        match worker {
            Ok(_) => {
                self.character_switch_rx = Some(rx);
                self.status = Some(Status::ok(format!(
                    "Logging out {current} and preparing {target}…"
                )));
            }
            Err(error) => {
                self.status = Some(Status::error(format!(
                    "Could not start the character-switch worker: {error}; {target} was not launched."
                )));
            }
        }
    }

    fn pump_character_switch(&mut self, ctx: &egui::Context) {
        let result = self.character_switch_rx.as_mut().map(|rx| rx.try_recv());
        match result {
            Some(Ok((profile, Ok(claim)))) => {
                self.character_switch_rx = None;
                let Some(command) = profile.custom_launch.clone() else {
                    self.status = Some(Status::error(format!(
                        "Character handoff completed, but {} has no Lich launch command; the target was not launched.",
                        profile.name
                    )));
                    return;
                };
                self.start_ssh_launch_reserved(&profile, &command, claim);
            }
            Some(Ok((profile, Err(error)))) => {
                self.character_switch_rx = None;
                self.status = Some(Status::error(format!(
                    "Could not switch to {}: {error}. The target was not launched.",
                    profile.character
                )));
            }
            Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                self.character_switch_rx = None;
                self.status = Some(Status::error(
                    "The character-switch worker ended without a result; the target was not launched.",
                ));
            }
            Some(Err(mpsc::error::TryRecvError::Empty)) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            None => {}
        }
    }

    fn start_stop(&mut self, entry: crate::core::session_registry::SessionEntry) {
        if self.launch_or_switch_busy() {
            self.status = Some(Status::error(
                "A launch, switch, or session stop request is already in progress.",
            ));
            return;
        }
        let character = entry.character.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        match std::thread::Builder::new()
            .name("session-stop".into())
            .spawn(move || {
                let result = crate::launcher::session_lifecycle::stop_inactive_runtime(&entry)
                    .map_err(|error| format!("{error:#}"));
                let _ = tx.send((character, result));
            }) {
            Ok(_) => {
                self.stop_progress_rx = Some(rx);
                self.status = Some(Status::ok("Stopping session…"));
            }
            Err(error) => {
                self.status = Some(Status::error(format!(
                    "Could not start session stop request: {error}"
                )));
            }
        }
    }

    fn pump_stop_progress(&mut self, ctx: &egui::Context) {
        let result = self.stop_progress_rx.as_mut().map(|rx| rx.try_recv());
        match result {
            Some(Ok((character, result))) => {
                self.stop_progress_rx = None;
                self.status = Some(match result {
                    Ok(()) => Status::ok(format!("Stopping {character} session…")),
                    Err(error) => {
                        Status::error(format!("Could not stop {character} session: {error}"))
                    }
                });
            }
            Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                self.stop_progress_rx = None;
                self.status = Some(Status::error(
                    "The session stop worker ended without a result.",
                ));
            }
            Some(Err(mpsc::error::TryRecvError::Empty)) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            None => {}
        }
    }

    // ----- saving the edit form --------------------------------------------

    fn commit_edit(&mut self) {
        if self.launch_or_switch_busy() {
            if let Some(form) = self.edit.as_mut() {
                form.error = Some(
                    "A launch or session lifecycle operation is in progress; this profile was not changed."
                        .to_string(),
                );
            }
            return;
        }
        let Some(form) = self.edit.as_mut() else {
            return;
        };

        let profile = &form.profile;
        let name = profile.name.trim();
        if name.is_empty() {
            form.error = Some("Profile name is required".to_string());
            return;
        }
        // Names ride through a cmd command line when spawning terminal
        // sessions on Windows; quotes and %-expansion cannot pass safely.
        if name.contains(['"', '%']) {
            form.error = Some("Profile name cannot contain \" or %".to_string());
            return;
        }
        let duplicate = self
            .store
            .find(name)
            .map(|existing| Some(existing.name.as_str()) != form.original_name.as_deref())
            .unwrap_or(false);
        if duplicate {
            form.error = Some(format!("A profile named '{name}' already exists"));
            return;
        }
        match profile.mode {
            LaunchMode::Direct => {
                if profile.account.trim().is_empty() || profile.character.trim().is_empty() {
                    form.error =
                        Some("Direct connections need an account and a character".to_string());
                    return;
                }
            }
            LaunchMode::Lich => {
                if profile.host.trim().is_empty() {
                    form.error = Some("Lich connections need a host".to_string());
                    return;
                }
            }
        }
        match form.port_text.trim().parse::<u16>() {
            Ok(port) if port != 0 => form.profile.port = port,
            _ if profile.mode == LaunchMode::Lich => {
                form.error = Some("Port must be a number between 1 and 65535".to_string());
                return;
            }
            _ => {}
        }
        if form.web_enabled {
            match form.web_port_text.trim().parse::<u16>() {
                Ok(port) if port != 0 => form.profile.web_port = Some(port),
                _ => {
                    form.error =
                        Some("Web dashboard port must be a number between 1 and 65535".to_string());
                    return;
                }
            }
            // Store the bind only when it differs from the loopback default,
            // so unchanged profiles keep a clean launcher.toml (None = 127.0.0.1).
            let bind = form.web_bind_text.trim();
            form.profile.web_bind = if bind.is_empty() || bind == "127.0.0.1" {
                None
            } else {
                Some(bind.to_string())
            };
        } else {
            form.profile.web_port = None;
            form.profile.web_bind = None;
        }
        // Lich-only, and blank means "attach only" — never persist an empty
        // string, which would read as a launch command that does nothing.
        form.profile.custom_launch = if form.profile.mode == LaunchMode::Lich {
            let cmd = form.custom_launch_text.trim();
            (!cmd.is_empty()).then(|| cmd.to_string())
        } else {
            None
        };

        let mut form = self.edit.take().expect("edit form present");
        form.profile.name = form.profile.name.trim().to_string();
        let mut keyring_warning = None;

        // Password bookkeeping (direct mode only). The keyring is keyed by
        // account, so renaming the account or unchecking "save" cleans up the
        // old entry unless another profile still relies on it.
        if form.profile.mode == LaunchMode::Direct {
            let account = form.profile.account.trim().to_string();
            form.profile.account = account.clone();

            if form.save_password && !form.password.is_empty() {
                match profiles::save_password(&account, &form.password) {
                    Ok(()) => form.profile.password_saved = true,
                    Err(err) => {
                        form.profile.password_saved = false;
                        keyring_warning = Some(format!(
                            "Profile saved, but the password was NOT stored ({err:#}). You will be asked for it at launch."
                        ));
                    }
                }
            } else if form.save_password
                && form.original_password_saved
                && account.eq_ignore_ascii_case(&form.original_account)
            {
                // Empty password field on an already-saved account = keep it.
                form.profile.password_saved = true;
            } else {
                form.profile.password_saved = false;
            }

            let dropped_old_entry = form.original_password_saved
                && (!form.profile.password_saved
                    || !account.eq_ignore_ascii_case(&form.original_account));
            if dropped_old_entry {
                let original_account = form.original_account.clone();
                let original_name = form.original_name.clone();
                let still_used = self.store.profiles.iter().any(|entry| {
                    Some(entry.name.as_str()) != original_name.as_deref()
                        && entry.password_saved
                        && entry.account.eq_ignore_ascii_case(&original_account)
                });
                if !still_used {
                    profiles::delete_password(&original_account);
                }
            }
        } else {
            form.profile.password_saved = false;
        }

        self.store
            .upsert(form.profile.clone(), form.original_name.as_deref());
        self.save_store();
        if let Some(warning) = keyring_warning {
            self.status = Some(Status::error(warning));
        } else if self.status.as_ref().map(|s| s.is_error) != Some(true) {
            self.status = Some(Status::ok(format!("Saved {}", form.profile.name)));
        }
    }

    fn delete_profile(&mut self, name: &str) {
        if self.launch_or_switch_busy() {
            self.status = Some(Status::error(
                "A launch or session lifecycle operation is in progress; the profile was not deleted.",
            ));
            return;
        }
        if let Some(removed) = self.store.remove(name) {
            if removed.password_saved && !self.store.account_password_in_use(&removed.account) {
                profiles::delete_password(&removed.account);
            }
            self.save_store();
            self.status = Some(Status::ok(format!("Deleted {}", removed.name)));
        }
    }

    // ----- UI --------------------------------------------------------------

    fn show_profile_list(&mut self, ui: &mut egui::Ui) {
        let mut launch_request = None;
        let mut edit_request = None;
        let mut delete_request = None;
        let mut stop_request = None;
        self.refresh_live_sessions_if_stale();
        let live_sessions = self.live_sessions.clone();
        let mutation_enabled = !self.launch_or_switch_busy();

        if self.store.profiles.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No saved connections yet").weak());
                ui.label(egui::RichText::new("Create one to get started").weak());
            });
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for profile in &self.store.profiles {
                    let identity =
                        crate::core::session_registry::SessionLaunchIdentity::from_profile(
                            &profile.name,
                            profile,
                        );
                    let disposition = crate::launcher::session_lifecycle::decide_launch(
                        &identity,
                        &live_sessions,
                    );
                    let (live, action) = match disposition {
                        crate::launcher::session_lifecycle::LaunchDisposition::Resume(entry) => {
                            (Some(entry), "Open")
                        }
                        crate::launcher::session_lifecycle::LaunchDisposition::Replace(entry) => {
                            (Some(entry), "Restart")
                        }
                        _ => (None, "Launch"),
                    };
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&profile.name).strong());
                                ui.label(egui::RichText::new(profile.summary()).weak().small());
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(mutation_enabled, egui::Button::new("Delete"))
                                        .clicked()
                                    {
                                        delete_request = Some(profile.name.clone());
                                    }
                                    if ui
                                        .add_enabled(mutation_enabled, egui::Button::new("Edit"))
                                        .clicked()
                                    {
                                        edit_request = Some(profile.name.clone());
                                    }
                                    if live.as_ref().is_some_and(|entry| {
                                        matches!(
                                            entry.lifecycle,
                                            crate::core::session_registry::SessionLifecycleState::Idle
                                                | crate::core::session_registry::SessionLifecycleState::Connecting
                                                | crate::core::session_registry::SessionLifecycleState::Reconnecting
                                                | crate::core::session_registry::SessionLifecycleState::Disconnected
                                        )
                                    }) && ui.button("Stop").clicked()
                                    {
                                        stop_request = live.clone();
                                    }
                                    if ui.button(egui::RichText::new(action).strong()).clicked() {
                                        launch_request = Some(profile.name.clone());
                                    }
                                },
                            );
                        });
                    });
                }
            });

        ui.add_space(8.0);
        if ui
            .add_enabled(mutation_enabled, egui::Button::new("➕ New connection"))
            .clicked()
        {
            self.edit = Some(EditForm::new_profile());
        }

        if let Some(name) = launch_request {
            self.launch(&name);
        }
        if let Some(name) = edit_request {
            if let Some(profile) = self.store.find(&name).cloned() {
                self.edit = Some(EditForm::edit(profile));
            }
        }
        if let Some(name) = delete_request {
            self.confirm_delete = Some(name);
        }
        if let Some(entry) = stop_request {
            self.start_stop(entry);
        }
    }

    fn show_edit_form(&mut self, ui: &mut egui::Ui) {
        let mut save_clicked = false;
        let mut cancel_clicked = false;

        {
            let form = self.edit.as_mut().expect("edit form present");
            let profile = &mut form.profile;

            ui.heading(if form.original_name.is_some() {
                "Edit connection"
            } else {
                "New connection"
            });
            ui.add_space(8.0);

            egui::Grid::new("launcher_edit_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut profile.name)
                            .hint_text("e.g. Nisugi - Prime"),
                    );
                    ui.end_row();

                    ui.label("Connection");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut profile.mode, LaunchMode::Direct, "Direct")
                            .on_hover_text(help::MODE_DIRECT);
                        ui.selectable_value(&mut profile.mode, LaunchMode::Lich, "Lich")
                            .on_hover_text(help::MODE_LICH);
                    });
                    ui.end_row();

                    match profile.mode {
                        LaunchMode::Direct => {
                            ui.label("Account").on_hover_text(help::ACCOUNT);
                            ui.add(egui::TextEdit::singleline(&mut profile.account));
                            ui.end_row();

                            ui.label("Password");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut form.password)
                                        .password(!form.show_password)
                                        .hint_text(if form.original_password_saved {
                                            "(saved - leave blank to keep)"
                                        } else {
                                            ""
                                        }),
                                );
                                let eye = if form.show_password { "🙈" } else { "👁" };
                                if ui
                                    .small_button(eye)
                                    .on_hover_text(if form.show_password {
                                        "Hide password"
                                    } else {
                                        "Show password"
                                    })
                                    .clicked()
                                {
                                    form.show_password = !form.show_password;
                                }
                            });
                            ui.end_row();

                            ui.label("");
                            ui.checkbox(&mut form.save_password, "Save password")
                                .on_hover_text(help::SAVE_PASSWORD);
                            ui.end_row();

                            ui.label("Game").on_hover_text(help::GAME);
                            let game_label = GAME_CHOICES
                                .iter()
                                .find(|(value, _)| *value == profile.game)
                                .map(|(_, label)| *label)
                                .unwrap_or("GemStone IV");
                            egui::ComboBox::from_id_salt("launcher_game")
                                .selected_text(game_label)
                                .show_ui(ui, |ui| {
                                    for (value, label) in GAME_CHOICES {
                                        ui.selectable_value(
                                            &mut profile.game,
                                            value.to_string(),
                                            *label,
                                        );
                                    }
                                });
                            ui.end_row();

                            ui.label("Character").on_hover_text(help::CHARACTER);
                            ui.add(egui::TextEdit::singleline(&mut profile.character));
                            ui.end_row();
                        }
                        LaunchMode::Lich => {
                            ui.label("Host").on_hover_text(help::HOST);
                            ui.add(egui::TextEdit::singleline(&mut profile.host));
                            ui.end_row();

                            ui.label("Port").on_hover_text(help::PORT);
                            ui.add(
                                egui::TextEdit::singleline(&mut form.port_text).desired_width(80.0),
                            );
                            ui.end_row();

                            ui.label("Character").on_hover_text(help::CHARACTER);
                            ui.add(egui::TextEdit::singleline(&mut profile.character));
                            ui.end_row();

                            // Same-machine profiles skip SSH entirely, so the
                            // hint below should not send the user off to
                            // configure a key they don't need.
                            let profile_is_local =
                                crate::launcher::flow::is_local_host(&profile.host);
                            ui.label("Launch command")
                                .on_hover_text(help::CUSTOM_LAUNCH);
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut form.custom_launch_text)
                                        .desired_rows(2)
                                        .hint_text(
                                            "rubyw lich.rbw --login NAME --detachable-client=8000",
                                        ),
                                );
                                ui.label(
                                    egui::RichText::new(if profile_is_local {
                                        "Optional. If set, Vellum runs this on THIS machine when \
                                         the port isn't already open. No SSH needed."
                                    } else {
                                        "Optional. If set, Vellum starts Lich over SSH when the \
                                         port isn't already open. SSH user/key: .launcher"
                                    })
                                    .small()
                                    .weak(),
                                );
                            });
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(8.0);
            egui::CollapsingHeader::new("Advanced")
                .default_open(false)
                .show(ui, |ui| {
                    egui::Grid::new("launcher_advanced_grid")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Frontend").on_hover_text(help::FRONTEND);
                            ui.horizontal(|ui| {
                                let native_gui = profile.web_client.is_none()
                                    && profile.frontend == LaunchFrontend::Gui;
                                if ui.selectable_label(native_gui, "GUI").clicked() {
                                    profile.select_frontend(LaunchFrontend::Gui);
                                }

                                let terminal = profile.web_client.is_none()
                                    && profile.frontend == LaunchFrontend::Tui;
                                if ui.selectable_label(terminal, "Terminal").clicked() {
                                    profile.select_frontend(LaunchFrontend::Tui);
                                }

                                let despana = LaunchWebClient::Despana;
                                if ui
                                    .selectable_label(
                                        profile.web_client == Some(despana),
                                        despana.label(),
                                    )
                                    .clicked()
                                {
                                    profile.select_web_client(despana);
                                }
                            });
                            ui.end_row();

                            ui.label("Web dashboard").on_hover_text(help::WEB_PORT);
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut form.web_enabled, "Enable on port");
                                ui.add_enabled(
                                    form.web_enabled,
                                    egui::TextEdit::singleline(&mut form.web_port_text)
                                        .desired_width(60.0),
                                );
                            });
                            ui.end_row();

                            ui.label("Bind address").on_hover_text(help::WEB_BIND);
                            ui.horizontal(|ui| {
                                ui.add_enabled(
                                    form.web_enabled,
                                    egui::TextEdit::singleline(&mut form.web_bind_text)
                                        .desired_width(120.0)
                                        .hint_text("127.0.0.1"),
                                )
                                .on_hover_text(help::WEB_BIND);
                                ui.label(
                                    egui::RichText::new("0.0.0.0 = allow LAN devices")
                                        .weak()
                                        .small(),
                                )
                                .on_hover_text(help::WEB_BIND);
                            });
                            ui.end_row();

                            ui.label("Sound");
                            ui.checkbox(&mut profile.nosound, "Disable sound")
                                .on_hover_text(help::NOSOUND);
                            ui.end_row();

                            ui.label("Settings profile")
                                .on_hover_text(help::SETTINGS_PROFILE);
                            let mut settings_profile =
                                profile.settings_profile.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut settings_profile)
                                        .hint_text("(character name)"),
                                )
                                .changed()
                            {
                                profile.settings_profile = if settings_profile.trim().is_empty() {
                                    None
                                } else {
                                    Some(settings_profile)
                                };
                            }
                            ui.end_row();

                            ui.label("Data directory").on_hover_text(help::DATA_DIR);
                            let mut data_dir = profile.data_dir.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut data_dir)
                                        .hint_text("~/.vellum-fe"),
                                )
                                .changed()
                            {
                                profile.data_dir = if data_dir.trim().is_empty() {
                                    None
                                } else {
                                    Some(data_dir)
                                };
                            }
                            ui.end_row();

                            if profile.web_client.is_none()
                                && profile.frontend == LaunchFrontend::Tui
                            {
                                ui.label("Color mode").on_hover_text(help::COLOR_MODE);
                                let selected = profile.color_mode.clone();
                                egui::ComboBox::from_id_salt("launcher_color_mode")
                                    .selected_text(selected.as_deref().unwrap_or("default"))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut profile.color_mode,
                                            None,
                                            "default",
                                        );
                                        ui.selectable_value(
                                            &mut profile.color_mode,
                                            Some("direct".to_string()),
                                            "direct",
                                        );
                                        ui.selectable_value(
                                            &mut profile.color_mode,
                                            Some("slot".to_string()),
                                            "slot",
                                        );
                                    });
                                ui.end_row();

                                ui.label("Palette");
                                ui.checkbox(&mut profile.setup_palette, "Set up on startup")
                                    .on_hover_text(help::SETUP_PALETTE);
                                ui.end_row();
                            }
                        });
                });

            if let Some(error) = &form.error {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("Save").strong()).clicked() {
                    save_clicked = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        }

        if save_clicked {
            self.commit_edit();
        }
        if cancel_clicked {
            self.edit = None;
        }
    }

    fn show_password_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.password_prompt.as_mut() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;

        egui::Window::new(format!("Password for {}", prompt.profile.name))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut prompt.password)
                            .password(!prompt.show_password)
                            .desired_width(220.0),
                    );
                    response.request_focus();
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    let eye = if prompt.show_password { "🙈" } else { "👁" };
                    if ui
                        .small_button(eye)
                        .on_hover_text(if prompt.show_password {
                            "Hide password"
                        } else {
                            "Show password"
                        })
                        .clicked()
                    {
                        prompt.show_password = !prompt.show_password;
                    }
                });
                ui.checkbox(&mut prompt.remember, "Save password")
                    .on_hover_text(help::SAVE_PASSWORD);
                ui.horizontal(|ui| {
                    if ui.button("Launch").clicked() {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            self.password_prompt = None;
            return;
        }
        if !submit {
            return;
        }

        let prompt = self.password_prompt.take().expect("prompt present");
        let profile = match prompt.verified_profile(&self.store) {
            Ok(profile) => profile,
            Err(error) => {
                self.status = Some(Status::error(error));
                return;
            }
        };
        if prompt.remember {
            match profiles::save_password(&profile.account, &prompt.password) {
                Ok(()) => {
                    if let Some(entry) = self
                        .store
                        .profiles
                        .iter_mut()
                        .find(|entry| entry.name == profile.name)
                    {
                        entry.password_saved = true;
                    }
                    self.save_store();
                }
                Err(err) => {
                    self.status = Some(Status::error(format!(
                        "Password was NOT stored ({err:#}); launching anyway."
                    )));
                }
            }
        }
        if let Some(claim) = prompt.launch_claim {
            self.spawn_reserved(&profile, Some(&prompt.password), Some(claim));
        } else {
            self.spawn(&profile, Some(&prompt.password));
        }
    }

    fn show_character_switch_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.character_switch_prompt.as_mut() else {
            return;
        };
        let current_character = prompt.request.current_character().to_string();
        let current_profile = prompt.request.current_profile().to_string();
        let target_character = prompt.request.target_character().to_string();
        let target_profile = prompt.request.target_profile().to_string();
        let host = prompt.request.host().to_string();
        let port = prompt.request.port();
        let mut confirm = false;
        let mut cancel = false;

        egui::Window::new("Switch Lich character?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "{current_character} ({current_profile}) currently uses Lich at {host}:{port}."
                ));
                ui.label(format!(
                    "Launching {target_character} ({target_profile}) will log out {current_character} before starting the new session."
                ));
                ui.add_space(6.0);
                ui.checkbox(&mut prompt.dont_warn_again, "Don't warn again");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new("Log out and switch").strong())
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            // Deliberately no persistence, shutdown request, or worker start.
            self.character_switch_prompt = None;
            return;
        }
        if !confirm {
            return;
        }

        let prompt = self
            .character_switch_prompt
            .take()
            .expect("character switch prompt present");
        if prompt.dont_warn_again && !self.store.skip_lich_switch_warning {
            self.store.skip_lich_switch_warning = true;
            if let Err(error) = self.store.save() {
                // The preference must reach disk before any logout I/O. Keep
                // the confirmation open so the user can retry or uncheck it.
                self.store.skip_lich_switch_warning = false;
                self.status = Some(Status::error(format!(
                    "Could not save 'Don't warn again': {error:#}. No session was logged out."
                )));
                self.character_switch_prompt = Some(prompt);
                return;
            }
        }
        self.start_character_switch(prompt.profile, prompt.request);
    }

    fn show_delete_confirm(&mut self, ctx: &egui::Context) {
        let Some(name) = self.confirm_delete.clone() else {
            return;
        };
        let mutation_enabled = !self.launch_or_switch_busy();
        let mut close = false;
        egui::Window::new("Delete profile?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Delete '{name}'? Its saved password is removed too (unless another profile uses the same account)."
                ));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            mutation_enabled,
                            egui::Button::new(egui::RichText::new("Delete").strong()),
                        )
                        .clicked()
                    {
                        self.delete_profile(&name);
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.confirm_delete = None;
        }
    }
}

impl eframe::App for LauncherApp {
    // This egui fork's App trait hands the root Ui instead of update(ctx).
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        // Advance any confirmed handoff and then the existing Lich launch
        // flow before painting this frame.
        self.pump_character_switch(ctx);
        self.pump_dormant_restart(ctx);
        self.pump_launch_progress(ctx);
        self.pump_stop_progress(ctx);
        reap_finished_session_children(&mut self.session_children);
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        egui::CentralPanel::default().show(root, |ui| {
            if self.edit.is_some() {
                self.show_edit_form(ui);
            } else {
                ui.heading("VellumFE");
                ui.label(egui::RichText::new("Choose a connection to launch").weak());
                ui.add_space(8.0);
                self.show_profile_list(ui);
            }

            if let Some(status) = &self.status {
                ui.add_space(10.0);
                let color = if status.is_error {
                    egui::Color32::from_rgb(220, 80, 80)
                } else {
                    egui::Color32::from_rgb(110, 190, 110)
                };
                ui.colored_label(color, &status.text);
            }
        });

        self.show_password_prompt(ctx);
        self.show_character_switch_prompt(ctx);
        self.show_delete_confirm(ctx);
    }
}

/// Boot the launcher window.
pub fn run_launcher() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("VellumFE Launcher")
            .with_inner_size([540.0, 620.0])
            .with_min_inner_size([420.0, 380.0]),
        ..Default::default()
    };
    eframe::run_native(
        "VellumFE Launcher",
        options,
        Box::new(|_cc| Ok(Box::new(LauncherApp::new()))),
    )
    .map_err(|err| anyhow!("Failed to run launcher: {}", err))
}

// ----- session spawning ----------------------------------------------------

/// Spawn a session process for a profile. `password` is only passed for
/// background direct sessions with nothing in the credential store; it travels
/// via a private environment variable, never argv.
fn spawn_session(
    exe: &std::path::Path,
    profile: &LauncherProfile,
    password: Option<&str>,
) -> Result<std::process::Child> {
    match session_spawn_kind(profile) {
        SessionSpawnKind::Background => background_session_command(exe, profile, password)
            .spawn()
            .with_context(|| format!("Failed to start session process from {}", exe.display())),
        SessionSpawnKind::Terminal => spawn_tui_session(exe, profile, password),
    }
}

fn reap_finished_session_children(children: &mut Vec<std::process::Child>) {
    children.retain_mut(|child| match child.try_wait() {
        Ok(Some(_)) => false,
        Ok(None) => true,
        Err(error) => {
            tracing::warn!(pid = child.id(), %error, "could not reap launcher child");
            true
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionSpawnKind {
    Background,
    Terminal,
}

fn session_spawn_kind(profile: &LauncherProfile) -> SessionSpawnKind {
    if profile.web_client.is_some() {
        SessionSpawnKind::Background
    } else {
        match profile.frontend {
            LaunchFrontend::Gui => SessionSpawnKind::Background,
            LaunchFrontend::Tui => SessionSpawnKind::Terminal,
        }
    }
}

/// Build the console-free child used by both native GUI and Despana sessions.
/// The profile name is the only launcher data on argv; a just-entered direct
/// password stays in the private environment handoff used by the existing GUI
/// path.
fn background_session_command(
    exe: &std::path::Path,
    profile: &LauncherProfile,
    password: Option<&str>,
) -> Command {
    let mut cmd = Command::new(exe);
    cmd.arg("--launch-profile")
        .arg(&profile.name)
        .arg("--launch-target-fingerprint")
        .arg(crate::launcher::session_lifecycle::launch_target_fingerprint(profile));
    if let Some(password) = password {
        cmd.env(profiles::PASSWORD_ENV, password);
    }
    // The launcher may have freed its console (double-click start), leaving
    // dead std handles behind. Default Stdio::inherit would try to duplicate
    // them and CreateProcess fails with os error 50 (rust-lang/rust#113277),
    // so hand the child explicit nulls. Sessions log to a file, never stdout.
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Console-subsystem exe: suppress the console entirely for background
        // children (the native GUI or browser is the only visible surface).
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Terminal sessions need a console/terminal of their own. A just-prompted
/// password rides along in the environment (it survives the cmd/start hop);
/// if the variable is missing the session falls back to a console prompt.
///
/// Spawning the session directly with CREATE_NEW_CONSOLE does not work from
/// a console-freed launcher: the child would inherit our dead std handles
/// (os error 50), and substituting NUL handles would leave the TUI writing
/// to NUL instead of its console. `cmd /c start` sidesteps both - the hidden
/// cmd owns a live (windowless) console, and `start` creates the session's
/// console fresh, with correct handles, visible.
#[cfg(windows)]
fn spawn_tui_session(
    exe: &std::path::Path,
    profile: &LauncherProfile,
    password: Option<&str>,
) -> Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // The profile name is interpolated into a cmd command line; names are
    // validated at save time, but never trust the file on disk.
    if profile.name.contains(['"', '%']) {
        return Err(anyhow!(
            "Profile name contains characters cmd cannot pass safely (\" or %)"
        ));
    }
    let exe = exe.display().to_string();
    if exe.contains(['"', '%']) {
        return Err(anyhow!("Executable path contains \" or %"));
    }

    let mut cmd = Command::new("cmd");
    if let Some(password) = password {
        cmd.env(profiles::PASSWORD_ENV, password);
    }
    cmd
        // First quoted token after `start` is the window title.
        .raw_arg(format!(
            "/c start \"VellumFE\" \"{}\" --launch-profile \"{}\" --launch-target-fingerprint \"{}\"",
            exe,
            profile.name,
            crate::launcher::session_lifecycle::launch_target_fingerprint(profile),
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("Failed to start terminal session")
}

#[cfg(target_os = "macos")]
fn spawn_tui_session(
    exe: &std::path::Path,
    profile: &LauncherProfile,
    _password: Option<&str>,
) -> Result<std::process::Child> {
    // No env handoff: `open` routes through LaunchServices, which does not
    // propagate our environment. The session prompts in Terminal instead.
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // Terminal.app runs .command files; generate one that execs the session.
    let script = format!(
        "#!/bin/sh\nexec {} --launch-profile {} --launch-target-fingerprint {}\n",
        shell_quote(&exe.display().to_string()),
        shell_quote(&profile.name),
        shell_quote(&crate::launcher::session_lifecycle::launch_target_fingerprint(profile)),
    );
    let path = std::env::temp_dir().join(format!("vellum-fe-{}.command", std::process::id()));
    let mut file = std::fs::File::create(&path)?;
    file.write_all(script.as_bytes())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Command::new("open")
        .arg(&path)
        .spawn()
        .context("Failed to open Terminal for the session")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_tui_session(
    exe: &std::path::Path,
    profile: &LauncherProfile,
    password: Option<&str>,
) -> Result<std::process::Child> {
    let exe = exe.display().to_string();
    // $TERMINAL first, then common emulators. gnome-terminal wants `--`,
    // the rest take `-e`-style trailing commands.
    let mut candidates: Vec<(String, Vec<&str>)> = Vec::new();
    if let Ok(term) = std::env::var("TERMINAL") {
        candidates.push((term, vec!["-e"]));
    }
    candidates.extend([
        ("x-terminal-emulator".to_string(), vec!["-e"]),
        ("gnome-terminal".to_string(), vec!["--"]),
        ("konsole".to_string(), vec!["-e"]),
        ("alacritty".to_string(), vec!["-e"]),
        ("kitty".to_string(), vec![]),
        ("xterm".to_string(), vec!["-e"]),
    ]);

    for (terminal, prefix) in candidates {
        let mut cmd = Command::new(&terminal);
        cmd.args(prefix)
            .arg(&exe)
            .arg("--launch-profile")
            .arg(&profile.name)
            .arg("--launch-target-fingerprint")
            .arg(crate::launcher::session_lifecycle::launch_target_fingerprint(profile));
        // Best-effort: factory-model emulators (gnome-terminal) do not
        // inherit our environment; the session prompts in its terminal then.
        if let Some(password) = password {
            cmd.env(crate::config::profiles::PASSWORD_ENV, password);
        }
        if let Ok(child) = cmd.spawn() {
            return Ok(child);
        }
    }
    Err(anyhow!(
        "No terminal emulator found. Run manually: {} --launch-profile '{}'",
        exe,
        profile.name
    ))
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switch_request(
        lifecycle: crate::core::session_registry::SessionLifecycleState,
    ) -> crate::launcher::session_lifecycle::SwitchRequest {
        use crate::core::session_registry::{
            SessionConnectionIdentity, SessionEntry, SessionLaunchIdentity,
        };
        let connection = SessionConnectionIdentity::Lich {
            host: "loopback".to_string(),
            port: 8000,
        };
        let current_identity = SessionLaunchIdentity {
            profile: "Briar".to_string(),
            character: "Briar".to_string(),
            connection: connection.clone(),
        };
        let current = SessionEntry {
            character: "Briar".to_string(),
            port: 8040,
            control_host: None,
            pid: 8040,
            started_at: "then".to_string(),
            instance_id: Some("instance-briar".to_string()),
            process_started_at: None,
            launch: Some(current_identity),
            data_root: Some(std::path::PathBuf::from("headless-data")),
            lifecycle,
        };
        let target = SessionLaunchIdentity {
            profile: "Aster".to_string(),
            character: "Aster".to_string(),
            connection,
        };
        match crate::launcher::session_lifecycle::decide_launch(&target, &[current]) {
            crate::launcher::session_lifecycle::LaunchDisposition::EndpointConflict(request) => {
                request
            }
            other => panic!("expected endpoint conflict, got {other:?}"),
        }
    }

    fn lich_profile_with_command(command: Option<&str>) -> LauncherProfile {
        let mut profile = LauncherProfile::new_direct();
        profile.mode = LaunchMode::Lich;
        profile.custom_launch = command.map(str::to_string);
        profile
    }

    fn direct_profile(name: &str, account: &str) -> LauncherProfile {
        let mut profile = LauncherProfile::new_direct();
        profile.name = name.to_string();
        profile.account = account.to_string();
        profile.character = name.to_string();
        profile
    }

    fn test_launcher_app(store: LauncherStore) -> LauncherApp {
        LauncherApp {
            store,
            edit: None,
            password_prompt: None,
            character_switch_prompt: None,
            confirm_delete: None,
            status: None,
            launch_progress_rx: None,
            pending_launch: None,
            pending_launch_claim: None,
            character_switch_rx: None,
            dormant_restart_rx: None,
            stop_progress_rx: None,
            live_sessions: Vec::new(),
            live_sessions_refreshed_at: std::time::Instant::now(),
            session_children: Vec::new(),
            session_executable: Ok(std::path::PathBuf::from("vellum-fe-test")),
        }
    }

    #[test]
    fn password_prompt_preserves_and_revalidates_launch_identity() {
        let original = direct_profile("Aster", "original-account");
        let prompt = PasswordPrompt::new(original.clone(), None);
        let mut store = LauncherStore::default();
        store.profiles.push(original.clone());

        assert_eq!(
            prompt.verified_profile(&store).unwrap().account,
            "original-account"
        );

        store.profiles[0].account = "different-account".to_string();
        assert!(prompt
            .verified_profile(&store)
            .unwrap_err()
            .contains("changed"));
        assert_eq!(prompt.profile.account, "original-account");

        store.profiles.clear();
        assert!(prompt
            .verified_profile(&store)
            .unwrap_err()
            .contains("deleted"));
    }

    #[test]
    fn profile_mutations_are_rejected_during_dormant_restart() {
        let original = direct_profile("Aster", "original-account");
        let mut store = LauncherStore::default();
        store.profiles.push(original.clone());
        let mut app = test_launcher_app(store);
        let (_tx, rx) = mpsc::unbounded_channel::<CharacterSwitchResult>();
        app.dormant_restart_rx = Some(rx);

        app.edit = Some(EditForm::edit(direct_profile("Aster", "changed-account")));
        app.commit_edit();
        assert_eq!(app.store.find("Aster").unwrap().account, "original-account");
        assert!(app.edit.as_ref().unwrap().error.is_some());

        app.delete_profile("Aster");
        assert!(app.store.find("Aster").is_some());
        assert!(app.status.as_ref().is_some_and(|status| status.is_error));
    }

    #[test]
    fn attach_only_conflicts_are_never_switchable() {
        let profile = lich_profile_with_command(None);
        let request =
            switch_request(crate::core::session_registry::SessionLifecycleState::Connected);

        assert_eq!(
            character_switch_action(&profile, &request, false),
            CharacterSwitchAction::RejectAttachOnly
        );
        assert_eq!(
            character_switch_action(&profile, &request, true),
            CharacterSwitchAction::RejectAttachOnly
        );

        let empty_command = lich_profile_with_command(Some("  "));
        assert_eq!(
            character_switch_action(&empty_command, &request, true),
            CharacterSwitchAction::RejectAttachOnly
        );
    }

    #[test]
    fn launch_capable_conflicts_respect_the_global_warning_preference() {
        let profile = lich_profile_with_command(Some("start-lich"));
        let request =
            switch_request(crate::core::session_registry::SessionLifecycleState::Connected);

        assert_eq!(
            character_switch_action(&profile, &request, false),
            CharacterSwitchAction::Confirm
        );
        assert_eq!(
            character_switch_action(&profile, &request, true),
            CharacterSwitchAction::Start
        );
    }

    #[test]
    fn native_owner_conflict_never_offers_destructive_handoff() {
        let profile = lich_profile_with_command(Some("start-lich"));
        let request = switch_request(crate::core::session_registry::SessionLifecycleState::Unknown);

        assert_eq!(
            character_switch_action(&profile, &request, false),
            CharacterSwitchAction::RejectUncontrollableOwner
        );
        assert_eq!(
            character_switch_action(&profile, &request, true),
            CharacterSwitchAction::RejectUncontrollableOwner
        );
    }

    #[test]
    fn despana_uses_the_console_free_background_spawn_path() {
        let mut profile = LauncherProfile::new_direct();
        profile.name = "Aster".to_string();
        profile.select_web_client(LaunchWebClient::Despana);

        assert_eq!(session_spawn_kind(&profile), SessionSpawnKind::Background);

        profile.select_frontend(LaunchFrontend::Gui);
        assert_eq!(session_spawn_kind(&profile), SessionSpawnKind::Background);

        profile.select_frontend(LaunchFrontend::Tui);
        assert_eq!(session_spawn_kind(&profile), SessionSpawnKind::Terminal);
    }

    #[test]
    fn background_spawn_keeps_password_off_argv() {
        let mut profile = LauncherProfile::new_direct();
        profile.name = "Aster".to_string();
        profile.select_web_client(LaunchWebClient::Despana);

        let command = background_session_command(
            std::path::Path::new("vellum-fe-test"),
            &profile,
            Some("not-on-argv"),
        );
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let fingerprint = crate::launcher::session_lifecycle::launch_target_fingerprint(&profile);
        assert_eq!(
            args,
            vec![
                "--launch-profile".to_string(),
                "Aster".to_string(),
                "--launch-target-fingerprint".to_string(),
                fingerprint,
            ]
        );
        assert!(!args.iter().any(|arg| arg.contains("not-on-argv")));

        let handed_off = command.get_envs().find_map(|(name, value)| {
            (name == std::ffi::OsStr::new(profiles::PASSWORD_ENV))
                .then(|| value.map(|value| value.to_string_lossy().into_owned()))
                .flatten()
        });
        assert_eq!(handed_off.as_deref(), Some("not-on-argv"));
    }

    #[cfg(unix)]
    #[test]
    fn retained_executable_path_survives_file_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("vellum-fe-test");
        let install = |path: &std::path::Path| {
            std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        install(&executable);

        // This is what LauncherApp captures at startup. Replacing the file
        // emulates `cargo build` unlinking the launcher's running image.
        let retained = executable.clone();
        std::fs::remove_file(&executable).unwrap();
        install(&executable);

        let mut profile = LauncherProfile::new_direct();
        profile.name = "Aster".to_string();
        profile.select_web_client(LaunchWebClient::Despana);
        let status = spawn_session(&retained, &profile, None)
            .unwrap()
            .wait()
            .unwrap();

        assert!(status.success());
    }

    #[cfg(unix)]
    #[test]
    fn finished_session_children_are_reaped() {
        let child = Command::new("sh")
            .args(["-c", "exit 0"])
            .spawn()
            .expect("short-lived child starts");
        let mut children = vec![child];

        for _ in 0..100 {
            reap_finished_session_children(&mut children);
            if children.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            children.is_empty(),
            "finished child must be waited and removed"
        );
    }
}
