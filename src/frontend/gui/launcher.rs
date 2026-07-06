//! Graphical launcher - saved connection profiles and session spawning.
//!
//! Shown when vellum-fe starts with no arguments (double-click) or with
//! `--launcher`. Each profile row spawns a *separate process* running
//! `vellum-fe --launch-profile NAME`, so a session goes through exactly the
//! same startup path as a hand-typed command line and several characters can
//! play at once.
//!
//! Passwords never appear on a child's command line. A session resolves its
//! own password from the OS credential store; only when nothing is saved
//! does the launcher prompt and hand the password over via a private
//! environment variable (GUI sessions) or let the session prompt in its own
//! console (terminal sessions).

use anyhow::{anyhow, Context, Result};
use eframe::egui;
use std::process::Command;

use crate::config::profiles::{
    self, help, LaunchFrontend, LaunchMode, LauncherProfile, LauncherStore, GAME_CHOICES,
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
    save_password: bool,
    web_enabled: bool,
    web_port_text: String,
    port_text: String,
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
            save_password: true,
            web_enabled: false,
            web_port_text: "8484".to_string(),
            port_text: "8000".to_string(),
            error: None,
        }
    }

    fn edit(profile: LauncherProfile) -> Self {
        Self {
            original_name: Some(profile.name.clone()),
            original_account: profile.account.clone(),
            original_password_saved: profile.password_saved,
            password: String::new(),
            save_password: profile.password_saved,
            web_enabled: profile.web_port.is_some(),
            web_port_text: profile.web_port.unwrap_or(8484).to_string(),
            port_text: profile.port.to_string(),
            profile,
            error: None,
        }
    }
}

/// Modal shown when launching a GUI direct profile with no saved password.
struct PasswordPrompt {
    profile_name: String,
    password: String,
    remember: bool,
}

pub struct LauncherApp {
    store: LauncherStore,
    edit: Option<EditForm>,
    password_prompt: Option<PasswordPrompt>,
    confirm_delete: Option<String>,
    status: Option<Status>,
}

impl LauncherApp {
    fn new() -> Self {
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
        Self {
            store,
            edit: None,
            password_prompt: None,
            confirm_delete: None,
            status,
        }
    }

    fn save_store(&mut self) {
        if let Err(err) = self.store.save() {
            self.status = Some(Status::error(format!("Failed to save profiles: {err:#}")));
        }
    }

    // ----- launching -------------------------------------------------------

    fn launch(&mut self, name: &str) {
        let Some(profile) = self.store.find(name).cloned() else {
            return;
        };
        match profile.mode {
            LaunchMode::Direct => {
                let saved = if profile.password_saved {
                    profiles::load_password(&profile.account)
                } else {
                    None
                };
                match (saved, profile.frontend) {
                    // Session re-reads the credential store itself.
                    (Some(_), _) => self.spawn(&profile, None),
                    // Terminal sessions get their own console and can prompt
                    // there, exactly like a hand-run --direct command.
                    (None, LaunchFrontend::Tui) => self.spawn(&profile, None),
                    // GUI sessions have no console to prompt in - collect the
                    // password here and hand it over privately.
                    (None, LaunchFrontend::Gui) => {
                        self.password_prompt = Some(PasswordPrompt {
                            profile_name: profile.name.clone(),
                            password: String::new(),
                            remember: false,
                        });
                    }
                }
            }
            LaunchMode::Lich => self.spawn(&profile, None),
        }
    }

    fn spawn(&mut self, profile: &LauncherProfile, password: Option<&str>) {
        match spawn_session(profile, password) {
            Ok(()) => self.status = Some(Status::ok(format!("Launched {}", profile.name))),
            Err(err) => {
                self.status = Some(Status::error(format!(
                    "Failed to launch {}: {err:#}",
                    profile.name
                )))
            }
        }
    }

    // ----- saving the edit form --------------------------------------------

    fn commit_edit(&mut self) {
        let Some(form) = self.edit.as_mut() else {
            return;
        };

        let profile = &form.profile;
        let name = profile.name.trim();
        if name.is_empty() {
            form.error = Some("Profile name is required".to_string());
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
        } else {
            form.profile.web_port = None;
        }

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
                                    if ui.button("Delete").clicked() {
                                        delete_request = Some(profile.name.clone());
                                    }
                                    if ui.button("Edit").clicked() {
                                        edit_request = Some(profile.name.clone());
                                    }
                                    if ui
                                        .button(egui::RichText::new("Launch").strong())
                                        .clicked()
                                    {
                                        launch_request = Some(profile.name.clone());
                                    }
                                },
                            );
                        });
                    });
                }
            });

        ui.add_space(8.0);
        if ui.button("➕ New connection").clicked() {
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
                            ui.add(
                                egui::TextEdit::singleline(&mut form.password)
                                    .password(true)
                                    .hint_text(if form.original_password_saved {
                                        "(saved - leave blank to keep)"
                                    } else {
                                        ""
                                    }),
                            );
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
                                egui::TextEdit::singleline(&mut form.port_text)
                                    .desired_width(80.0),
                            );
                            ui.end_row();

                            ui.label("Character").on_hover_text(help::CHARACTER);
                            ui.add(egui::TextEdit::singleline(&mut profile.character));
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
                                ui.selectable_value(
                                    &mut profile.frontend,
                                    LaunchFrontend::Gui,
                                    "GUI",
                                );
                                ui.selectable_value(
                                    &mut profile.frontend,
                                    LaunchFrontend::Tui,
                                    "Terminal",
                                );
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

                            if profile.frontend == LaunchFrontend::Tui {
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

        egui::Window::new(format!("Password for {}", prompt.profile_name))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut prompt.password)
                        .password(true)
                        .desired_width(220.0),
                );
                response.request_focus();
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
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
        let Some(profile) = self.store.find(&prompt.profile_name).cloned() else {
            return;
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
        self.spawn(&profile, Some(&prompt.password));
    }

    fn show_delete_confirm(&mut self, ctx: &egui::Context) {
        let Some(name) = self.confirm_delete.clone() else {
            return;
        };
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
                    if ui.button(egui::RichText::new("Delete").strong()).clicked() {
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
        egui::CentralPanel::default().show_inside(root, |ui| {
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

/// Spawn a session process for a profile. `password` is only passed for GUI
/// direct sessions with nothing in the credential store; it travels via a
/// private environment variable, never argv.
fn spawn_session(profile: &LauncherProfile, password: Option<&str>) -> Result<()> {
    let exe = std::env::current_exe().context("Could not locate the vellum-fe executable")?;

    match profile.frontend {
        LaunchFrontend::Gui => {
            let mut cmd = Command::new(&exe);
            cmd.arg("--launch-profile").arg(&profile.name);
            if let Some(password) = password {
                cmd.env(profiles::PASSWORD_ENV, password);
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                // Console-subsystem exe: suppress the console entirely for
                // GUI children (the egui window is the only surface).
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
            cmd.spawn().context("Failed to start session process")?;
            Ok(())
        }
        LaunchFrontend::Tui => spawn_tui_session(&exe, profile),
    }
}

/// Terminal sessions need a console/terminal of their own. They never get a
/// password handoff: with a console available, the session can prompt there.
#[cfg(windows)]
fn spawn_tui_session(exe: &std::path::Path, profile: &LauncherProfile) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    Command::new(exe)
        .arg("--launch-profile")
        .arg(&profile.name)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .context("Failed to start terminal session")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn spawn_tui_session(exe: &std::path::Path, profile: &LauncherProfile) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // Terminal.app runs .command files; generate one that execs the session.
    let script = format!(
        "#!/bin/sh\nexec {} --launch-profile {}\n",
        shell_quote(&exe.display().to_string()),
        shell_quote(&profile.name),
    );
    let path = std::env::temp_dir().join(format!("vellum-fe-{}.command", std::process::id()));
    let mut file = std::fs::File::create(&path)?;
    file.write_all(script.as_bytes())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Command::new("open")
        .arg(&path)
        .spawn()
        .context("Failed to open Terminal for the session")?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_tui_session(exe: &std::path::Path, profile: &LauncherProfile) -> Result<()> {
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
            .arg(&profile.name);
        if cmd.spawn().is_ok() {
            return Ok(());
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
