//! Connection lifecycle: manual reconnect, launching a character via
//! eAccess or Lich, launch-progress pumping, and Lich attach.

use super::*;

impl VellumGuiApp {
    /// Re-establish the game connection after a drop (`.reconnect` or the
    /// toolbar Reconnect button). A single manual attempt: rebuild the command
    /// channel, spawn a fresh network task from the retained inputs (direct
    /// re-auths; Lich re-attaches), and let the server's login snapshot refresh
    /// game state in place. Existing state is left on screen until it arrives.
    pub(super) fn reconnect(&mut self) {
        if self.app_core.game_state.connected {
            self.app_core
                .add_system_message("Already connected — nothing to reconnect.");
            return;
        }

        // Drop the old (dead) network task; its socket half is already gone,
        // but abort so a half-open task can't linger.
        if let Some(handle) = self.network_handle.take() {
            handle.abort();
        }

        // Fresh command channel: the old command_rx was moved into the dead
        // task. Swap in the new sender so typed commands reach the new socket.
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();
        self.command_tx = command_tx;

        // A fresh raw logger (owns a background writer thread + file handle);
        // the old one drops with the previous task and flushes.
        let raw_logger = match RawLogger::new(&self.app_core.config) {
            Ok(logger) => logger,
            Err(err) => {
                tracing::error!("Reconnect: failed to init raw logger: {}", err);
                None
            }
        };

        let server_tx = self.network_forward_tx.clone();
        // Re-arm the layout-driven WebUI handshake for the new session.
        self.webui_handshake_sent = false;

        let handle = match self.reconnect_direct.clone() {
            Some(cfg) => self._runtime.spawn(async move {
                if let Err(err) =
                    crate::network::DirectConnection::start(cfg, server_tx, command_rx, raw_logger)
                        .await
                {
                    tracing::error!("GUI reconnect (direct) error: {}", err);
                }
            }),
            None => {
                let host = self.reconnect_host.clone();
                let port = self.reconnect_port;
                let login_key = self.reconnect_login_key.clone();
                self._runtime.spawn(async move {
                    if let Err(err) = LichConnection::start(
                        &host, port, login_key, server_tx, command_rx, raw_logger,
                    )
                    .await
                    {
                        tracing::error!("GUI reconnect (lich) error: {}", err);
                    }
                })
            }
        };
        self.network_handle = Some(handle);
        self.app_core.add_system_message("Reconnecting…");
        self.app_core.needs_render = true;
    }

    /// Kick off the SSH launcher for `character`. The flow (SSH connect +
    /// remote spawn + port poll) can take several seconds, so it runs on the
    /// tokio runtime and reports progress over an unbounded channel we drain
    /// each frame in [`Self::pump_launch_progress`]. On `Ready` we attach to
    /// the resulting Lich target exactly like a reconnect.
    pub(super) fn start_launch(&mut self, character: &str) {
        if self.app_core.game_state.connected {
            self.app_core
                .add_system_message("Already connected — disconnect before launching a session.");
            return;
        }
        let character = character.trim().to_string();
        if character.is_empty() {
            self.open_launcher_editor();
            return;
        }

        let config = match crate::launcher::config::LauncherConfig::load() {
            Ok(cfg) => cfg,
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Launcher config error: {err:#}"));
                return;
            }
        };
        if config.character(&character).is_none() {
            self.app_core.add_system_message(&format!(
                "No launcher entry for '{character}'. Open the launcher editor with .launcher."
            ));
            return;
        }

        let (tx, rx) = mpsc::unbounded_channel::<crate::launcher::flow::LaunchProgress>();
        self.launch_progress_rx = Some(rx);
        self.app_core
            .add_system_message(&format!("Launching {character}…"));
        self.app_core.needs_render = true;

        // Host-key trust: over an already-private tunnel with no interactive
        // prompt wired into the flow task, auto-pin on first use. A changed key
        // is still hard-rejected inside the flow.
        let trust = crate::launcher::flow::HostKeyTrust::AutoPinFirstUse;
        // russh's `Handle` is not provably Send across await points, so the
        // flow can't be spawned onto the shared multi-thread runtime. Run it on
        // a dedicated OS thread with its own current-thread runtime instead;
        // progress still flows back over the channel to the egui loop.
        std::thread::Builder::new()
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
                    let _ = crate::launcher::flow::launch(&config, &character, trust, |p| {
                        let _ = tx_progress.send(p);
                    })
                    .await;
                });
            })
            .ok();
    }

    /// Drain launcher progress each frame: surface messages, and on `Ready`
    /// attach to the Lich target. Called from the egui update loop.
    pub(super) fn pump_launch_progress(&mut self) {
        use crate::launcher::flow::LaunchProgress as LP;
        let mut ready: Option<(String, u16)> = None;
        let mut drained_any = false;
        if let Some(rx) = self.launch_progress_rx.as_mut() {
            while let Ok(progress) = rx.try_recv() {
                drained_any = true;
                match progress {
                    LP::Resolving { character } => self
                        .app_core
                        .add_system_message(&format!("Resolving {character}…")),
                    LP::AlreadyRunning { host, port } => self.app_core.add_system_message(
                        &format!("Lich already running at {host}:{port} — attaching."),
                    ),
                    LP::Connecting { host, port } => self
                        .app_core
                        .add_system_message(&format!("Connecting to {host}:{port} over SSH…")),
                    LP::HostKeyPrompt { fingerprint } => self.app_core.add_system_message(
                        &format!("Pinned new host key {fingerprint} (first use)."),
                    ),
                    LP::HostKeyChanged => self.app_core.add_system_message(
                        "⚠ Host key CHANGED — refusing to connect (possible MITM). \
                         Clear ssh-launcher-known-hosts only if you trust the change.",
                    ),
                    LP::Spawning { character } => self
                        .app_core
                        .add_system_message(&format!("Starting headless Lich for {character}…")),
                    LP::WaitingForPort { host, port } => self
                        .app_core
                        .add_system_message(&format!("Waiting for Lich on {host}:{port}…")),
                    LP::Ready { host, port } => {
                        self.app_core
                            .add_system_message(&format!("Lich up at {host}:{port} — attaching."));
                        ready = Some((host, port));
                    }
                    LP::Failed { reason } => self
                        .app_core
                        .add_system_message(&format!("Launch failed: {reason}")),
                }
            }
        }
        if drained_any {
            self.app_core.needs_render = true;
        }
        if let Some((host, port)) = ready {
            self.launch_progress_rx = None;
            self.attach_lich(host, port);
        }
    }

    /// Attach to a headless Lich in detachable-client mode at `host:port`.
    /// Mirrors the Lich branch of [`Self::reconnect`], and records the target
    /// so a later `.reconnect` re-attaches to the same session.
    pub(super) fn attach_lich(&mut self, host: String, port: u16) {
        if let Some(handle) = self.network_handle.take() {
            handle.abort();
        }
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();
        self.command_tx = command_tx;
        let raw_logger = RawLogger::new(&self.app_core.config).ok().flatten();
        let server_tx = self.network_forward_tx.clone();
        self.webui_handshake_sent = false;

        // Remember this as the reconnect target (detachable Lich, no key).
        self.reconnect_direct = None;
        self.reconnect_login_key = None;
        self.reconnect_host = host.clone();
        self.reconnect_port = port;

        let handle = self._runtime.spawn(async move {
            if let Err(err) =
                LichConnection::start(&host, port, None, server_tx, command_rx, raw_logger).await
            {
                tracing::error!("GUI launch attach (lich) error: {}", err);
            }
        });
        self.network_handle = Some(handle);
        self.app_core.needs_render = true;
    }

}
