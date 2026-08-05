//! GUI editor for the SSH launcher (`.launcher`).
//!
//! Edits `ssh-launcher.toml` — the SSH target, launch-command template, and
//! per-character ports — plus the dedicated ed25519 key: generate it, then
//! show the one-line `authorized_keys` entry to paste onto the home PC. The
//! private key is written to the OS secure store, never to the TOML.
//!
//! Honors the project's no-hand-editing rule: everything a `.launch` needs is
//! set here, with copy buttons for the public key and a hardened-authorized_keys
//! hint so a leaked key can be restricted to launching the game.

use eframe::egui;

use super::super::VellumGuiApp;
use crate::launcher::config::{
    CharacterLaunch, LauncherConfig, RemoteOs, DEFAULT_KEY_ID,
};

pub(in super::super) struct LauncherEditorState {
    /// Working copy of the whole config; applied to disk on Save.
    working: LauncherConfig,
    dirty: bool,
    error: Option<String>,
    status: Option<String>,
    /// The public `authorized_keys` line to display after generate / on load,
    /// so the user can copy it. None if no key is stored yet.
    public_key_line: Option<String>,
    /// New-character form fields.
    new_char_name: String,
    new_char_game: String,
    new_char_port: u16,
}

impl LauncherEditorState {
    fn from_config(config: LauncherConfig) -> Self {
        // If a key is already stored, surface its public line for copying.
        let public_key_line = if config.ssh.key_saved {
            crate::launcher::config::load_private_key(DEFAULT_KEY_ID)
                .and_then(|pem| crate::launcher::ssh::public_line_from_private(&pem).ok())
        } else {
            None
        };
        Self {
            working: config,
            dirty: false,
            error: None,
            status: None,
            public_key_line,
            new_char_name: String::new(),
            new_char_game: "gemstone".to_string(),
            new_char_port: 8001,
        }
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_launcher_editor(&mut self) {
        if self.launcher_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_launcher_editor"));
            return;
        }
        let config = LauncherConfig::load().unwrap_or_default();
        self.launcher_editor = Some(LauncherEditorState::from_config(config));
    }

    pub(in super::super) fn render_launcher_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.launcher_editor.take() else {
            return;
        };

        let mut open = true;
        let mut save_requested = false;
        let mut generate_requested = false;
        let mut copy_public_key = false;
        let mut delete_char: Option<String> = None;
        let mut add_char = false;

        egui::Window::new("SSH Launcher")
            .id(egui::Id::new("gui_launcher_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(640.0)
            .default_height(560.0)
            .show(ctx, |ui| {
                if let Some(error) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if let Some(status) = &state.status {
                    ui.colored_label(ui.visuals().weak_text_color(), status);
                }

                ui.label(
                    "Cold-start a headless Lich on your home PC over your WireGuard/Tailscale \
                     tunnel, then attach. Run it with .launch <character>.",
                );
                ui.separator();

                // ---- SSH target ------------------------------------------
                ui.heading("Home PC (SSH)");
                egui::Grid::new("launcher_ssh_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Host (tunnel address)");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut state.working.ssh.host)
                                    .hint_text("100.x.x.x or hostname")
                                    .desired_width(280.0),
                            )
                            .changed()
                        {
                            state.dirty = true;
                        }
                        ui.end_row();

                        ui.label("User");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut state.working.ssh.user)
                                    .desired_width(280.0),
                            )
                            .changed()
                        {
                            state.dirty = true;
                        }
                        ui.end_row();

                        ui.label("SSH port");
                        if ui
                            .add(egui::DragValue::new(&mut state.working.ssh.port).range(1..=65535))
                            .changed()
                        {
                            state.dirty = true;
                        }
                        ui.end_row();

                        ui.label("Remote OS");
                        egui::ComboBox::from_id_salt("launcher_remote_os")
                            .selected_text(match state.working.ssh.remote_os {
                                RemoteOs::Windows => "Windows",
                                RemoteOs::Unix => "Unix/macOS",
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_value(
                                        &mut state.working.ssh.remote_os,
                                        RemoteOs::Windows,
                                        "Windows",
                                    )
                                    .clicked()
                                {
                                    state.dirty = true;
                                }
                                if ui
                                    .selectable_value(
                                        &mut state.working.ssh.remote_os,
                                        RemoteOs::Unix,
                                        "Unix/macOS",
                                    )
                                    .clicked()
                                {
                                    state.dirty = true;
                                }
                            });
                        ui.end_row();

                        ui.label("Attach host")
                            .on_hover_text("Where to attach after launch. Blank = same as Host.");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut state.working.ssh.attach_host)
                                    .hint_text("(same as host)")
                                    .desired_width(280.0),
                            )
                            .changed()
                        {
                            state.dirty = true;
                        }
                        ui.end_row();
                    });

                ui.add_space(6.0);
                ui.label("Launch command template")
                    .on_hover_text("{character}, {game}, {port} are substituted per character.");
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut state.working.ssh.lich_command)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .hint_text(
                                "C:/Ruby4Lich5/4.0.3/bin/rubyw.exe C:/Gemstone/dev/lich-5/lich.rbw \
                                 --login {character} --{game} --without-frontend \
                                 --bind-address=lan --detachable-client={port}",
                            ),
                    )
                    .changed()
                {
                    state.dirty = true;
                }

                ui.separator();

                // ---- Launcher key ----------------------------------------
                ui.heading("Launcher key");
                ui.label(
                    "A dedicated ed25519 key. Generate it here, then paste the public line into \
                     ~/.ssh/authorized_keys on the home PC. The private key is stored in your OS \
                     secure store — never in a file.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Generate new key").clicked() {
                        generate_requested = true;
                    }
                    if state.working.ssh.key_saved {
                        ui.colored_label(
                            ui.visuals().hyperlink_color,
                            "✓ key stored",
                        );
                    } else {
                        ui.weak("no key yet");
                    }
                });

                if let Some(public_line) = &state.public_key_line {
                    ui.add_space(4.0);
                    ui.label("Public key (paste into authorized_keys):");
                    let mut shown = public_line.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut shown)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .interactive(false)
                            .font(egui::TextStyle::Monospace),
                    );
                    if ui.button("Copy public key").clicked() {
                        copy_public_key = true;
                    }
                    ui.collapsing("Harden it (recommended)", |ui| {
                        ui.label(
                            "Restrict the key so a leak can only launch the game, not open a \
                             shell. Prefix the authorized_keys line with:",
                        );
                        let mut hint =
                            "restrict,command=\"...your exact lich launch...\" ".to_string();
                        ui.add(
                            egui::TextEdit::singleline(&mut hint)
                                .interactive(false)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });
                }

                ui.separator();

                // ---- Characters ------------------------------------------
                ui.heading("Characters");
                egui::Grid::new("launcher_char_grid")
                    .num_columns(4)
                    .spacing([12.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Character");
                        ui.strong("Game token");
                        ui.strong("Port");
                        ui.strong("");
                        ui.end_row();

                        // Collect names first so we can mutate the map while
                        // iterating a snapshot.
                        let names: Vec<String> =
                            state.working.characters.keys().cloned().collect();
                        for name in names {
                            ui.label(&name);
                            if let Some(entry) = state.working.characters.get_mut(&name) {
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut entry.game)
                                            .desired_width(120.0),
                                    )
                                    .changed()
                                {
                                    state.dirty = true;
                                }
                                if ui
                                    .add(egui::DragValue::new(&mut entry.port).range(1..=65535))
                                    .changed()
                                {
                                    state.dirty = true;
                                }
                            }
                            if ui.button("Remove").clicked() {
                                delete_char = Some(name.clone());
                            }
                            ui.end_row();
                        }
                    });

                ui.add_space(6.0);
                ui.label("Add character");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_char_name)
                            .hint_text("name")
                            .desired_width(140.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_char_game)
                            .hint_text("gemstone")
                            .desired_width(110.0),
                    );
                    ui.add(egui::DragValue::new(&mut state.new_char_port).range(1..=65535));
                    if ui.button("Add").clicked() {
                        add_char = true;
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    let save = ui.add_enabled(
                        state.dirty,
                        egui::Button::new(if state.dirty { "Save*" } else { "Save" }),
                    );
                    if save.clicked() {
                        save_requested = true;
                    }
                    ui.weak(
                        "Saves ssh-launcher.toml (no secrets); the key lives in the OS store.",
                    );
                });
            });

        // ---- Apply out-of-closure actions ----------------------------------
        if add_char {
            let name = state.new_char_name.trim().to_string();
            if name.is_empty() {
                state.error = Some("Character name is required.".to_string());
            } else if state.working.character(&name).is_some() {
                state.error = Some(format!("'{name}' already has an entry."));
            } else {
                state.working.characters.insert(
                    name,
                    CharacterLaunch {
                        game: if state.new_char_game.trim().is_empty() {
                            "gemstone".to_string()
                        } else {
                            state.new_char_game.trim().to_string()
                        },
                        port: state.new_char_port,
                    },
                );
                state.new_char_name.clear();
                state.dirty = true;
                state.error = None;
            }
        }

        if let Some(name) = delete_char {
            state.working.characters.remove(&name);
            state.dirty = true;
        }

        if generate_requested {
            let comment = format!(
                "vellum-launcher@{}",
                whoami_label()
            );
            match crate::launcher::ssh::generate_keypair(&comment) {
                Ok(key) => {
                    match crate::launcher::config::save_private_key(
                        DEFAULT_KEY_ID,
                        &key.private_openssh,
                    ) {
                        Ok(()) => {
                            state.working.ssh.key_saved = true;
                            state.public_key_line = Some(key.public_authorized_keys.clone());
                            state.dirty = true;
                            state.error = None;
                            state.status = Some(
                                "New key generated — copy the public line to the home PC's \
                                 authorized_keys."
                                    .to_string(),
                            );
                        }
                        Err(e) => {
                            state.error = Some(format!("Could not store key: {e:#}"));
                        }
                    }
                }
                Err(e) => state.error = Some(format!("Key generation failed: {e:#}")),
            }
        }

        if copy_public_key {
            if let Some(line) = &state.public_key_line {
                ctx.copy_text(line.clone());
                state.status = Some("Public key copied to clipboard.".to_string());
            }
        }

        if save_requested {
            match state.working.save() {
                Ok(()) => {
                    state.dirty = false;
                    state.error = None;
                    state.status = Some("Saved.".to_string());
                    self.app_core.add_system_message("SSH launcher config saved.");
                }
                Err(e) => state.error = Some(format!("Save failed: {e:#}")),
            }
        }

        if open {
            self.launcher_editor = Some(state);
        }
    }
}

/// A short label for the key comment. Best-effort; falls back to "vellum".
fn whoami_label() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "vellum".to_string())
}
