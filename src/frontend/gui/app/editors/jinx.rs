//! Jinx asset-manager panel: a native egui window over VellumFE's own Jinx
//! client (no jinx.lic, no GTK). Mirrors jinx.lic's GUI minus Scripts (Vellum
//! doesn't script): a Log tab, a Repos tab (list/add/remove), and an Assets
//! tab with category sub-tabs (Data / Skins / Layouts / images / sounds / …).
//!
//! The catalog is fetched off-thread by the shared JinxWorker (Request::Catalog
//! → Effect::Catalog, stashed on AppCore); this panel renders it and drives
//! install/update/refresh through the same worker. One job at a time (the
//! worker enforces it); buttons disable while in flight.

use super::super::VellumGuiApp;
use crate::core::jinx::worker::{CatalogEntry, Request};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum JinxTab {
    Log,
    Repos,
    Assets,
}

pub(in super::super) struct JinxPanelState {
    tab: JinxTab,
    /// Selected category sub-tab in Assets (a `kind` string), None = all.
    category: Option<String>,
    /// Streaming worker output shown in the Log tab.
    log: Vec<String>,
    /// Repo add form.
    new_repo_name: String,
    new_repo_url: String,
    repo_error: Option<String>,
    /// Set once so the panel kicks a catalog refresh the first frame it opens.
    requested_initial: bool,
}

impl Default for JinxPanelState {
    fn default() -> Self {
        Self {
            tab: JinxTab::Assets,
            category: None,
            log: Vec::new(),
            new_repo_name: String::new(),
            new_repo_url: String::new(),
            repo_error: None,
            requested_initial: false,
        }
    }
}

/// Friendlier category labels for the known kinds; unknown kinds show raw.
fn kind_label(kind: &str) -> &str {
    match kind {
        "data" => "Data",
        "skin" => "Skins",
        "layout" | "uipack" => "Layouts",
        "statusicon" => "Status icons",
        "compass" => "Compass",
        "hand" => "Hands",
        "doll" => "Injury doll",
        "frame" => "Frames",
        "background" => "Backgrounds",
        "icon" | "iconmap" | "image" => "Icons",
        "sound" => "Sounds",
        other => other,
    }
}

impl VellumGuiApp {
    /// Add a repo to repos.toml (same path `.jinx repo add` uses).
    fn jinx_repo_add(&mut self, name: &str, url: &str) -> Result<(), String> {
        let mut list = crate::core::jinx::repo::RepoList::load_or_seed(self.app_core.game_type())
            .map_err(|e| format!("cannot load repos: {e}"))?;
        list.add(name, url).map_err(|e| e.to_string())?;
        list.save().map_err(|e| format!("save failed: {e}"))
    }

    /// Remove a repo from repos.toml.
    fn jinx_repo_remove(&mut self, name: &str) -> Result<(), String> {
        let mut list = crate::core::jinx::repo::RepoList::load_or_seed(self.app_core.game_type())
            .map_err(|e| format!("cannot load repos: {e}"))?;
        list.remove(name).map_err(|e| e.to_string())?;
        list.save().map_err(|e| format!("save failed: {e}"))
    }

    pub(in super::super) fn open_jinx_panel(&mut self) {
        if self.jinx_panel.is_some() {
            self.raise_editor(egui::Id::new("gui_jinx_panel"));
            return;
        }
        self.jinx_panel = Some(JinxPanelState::default());
    }

    pub(super) fn render_jinx_panel(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.jinx_panel.take() else {
            return;
        };

        // Drain worker output into the panel log (poll_jinx already ran this
        // frame and routed lines to system messages + the catalog effect; we
        // mirror the recent lines here for the Log tab). Kick an initial
        // catalog fetch the first frame.
        if !state.requested_initial {
            state.requested_initial = true;
            let ack = self.app_core.jinx_worker.start(Request::Catalog);
            state.log.push(ack);
        }

        let in_flight = self.app_core.jinx_worker.in_flight();
        let catalog: Vec<CatalogEntry> = self.app_core.jinx_catalog.clone().unwrap_or_default();

        // Categories present in the catalog, in a stable friendly order.
        const KIND_ORDER: &[&str] = &[
            "data", "skin", "layout", "uipack", "statusicon", "compass", "hand", "doll", "frame",
            "background", "icon", "iconmap", "image", "sound",
        ];
        let mut kinds: Vec<String> = Vec::new();
        for entry in &catalog {
            if !kinds.iter().any(|k| k == &entry.kind) {
                kinds.push(entry.kind.clone());
            }
        }
        kinds.sort_by_key(|k| {
            KIND_ORDER
                .iter()
                .position(|o| o == k)
                .unwrap_or(usize::MAX)
        });

        let mut open = true;
        let mut refresh = false;
        let mut update_all = false;
        let mut install: Option<(String, String, bool)> = None; // (name, repo, overwrite)
        let mut repo_add: Option<(String, String)> = None;
        let mut repo_remove: Option<String> = None;

        egui::Window::new("Jinx - Asset Manager")
            .id(egui::Id::new("gui_jinx_panel"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(560.0)
            .default_height(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.tab, JinxTab::Assets, "Assets");
                    ui.selectable_value(&mut state.tab, JinxTab::Repos, "Repos");
                    ui.selectable_value(&mut state.tab, JinxTab::Log, "Log");
                    ui.separator();
                    if ui
                        .add_enabled(!in_flight, egui::Button::new("Refresh"))
                        .on_hover_text("Re-fetch the catalog from all repos")
                        .clicked()
                    {
                        refresh = true;
                    }
                    if in_flight {
                        ui.spinner();
                        ui.weak("working…");
                    }
                });
                ui.separator();

                match state.tab {
                    JinxTab::Assets => {
                        if catalog.is_empty() {
                            ui.weak(if in_flight {
                                "Loading catalog…"
                            } else {
                                "No assets — Refresh to fetch, or check Repos."
                            });
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                let all = state.category.is_none();
                                if ui.selectable_label(all, "All").clicked() {
                                    state.category = None;
                                }
                                for kind in &kinds {
                                    let sel = state.category.as_deref() == Some(kind.as_str());
                                    if ui.selectable_label(sel, kind_label(kind)).clicked() {
                                        state.category = Some(kind.clone());
                                    }
                                }
                            });
                            if ui
                                .add_enabled(!in_flight, egui::Button::new("Update all installed"))
                                .clicked()
                            {
                                update_all = true;
                            }
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    egui::Grid::new("jinx_assets_grid")
                                        .num_columns(5)
                                        .striped(true)
                                        .show(ui, |ui| {
                                            ui.strong("Name");
                                            ui.strong("Repo");
                                            ui.strong("Version");
                                            ui.strong("Status");
                                            ui.label("");
                                            ui.end_row();
                                            for entry in &catalog {
                                                if let Some(cat) = &state.category {
                                                    if &entry.kind != cat {
                                                        continue;
                                                    }
                                                }
                                                ui.label(entry.title.as_deref().unwrap_or(&entry.name))
                                                    .on_hover_text(&entry.name);
                                                ui.weak(&entry.repo);
                                                ui.weak(entry.version.as_deref().unwrap_or("—"));
                                                if entry.update_available {
                                                    ui.colored_label(
                                                        ui.visuals().warn_fg_color,
                                                        "update",
                                                    );
                                                } else if entry.installed {
                                                    ui.weak("installed");
                                                } else {
                                                    ui.weak("—");
                                                }
                                                let (label, overwrite) = if entry.update_available {
                                                    ("Update", true)
                                                } else if entry.installed {
                                                    ("Reinstall", true)
                                                } else {
                                                    ("Install", false)
                                                };
                                                if ui
                                                    .add_enabled(
                                                        !in_flight,
                                                        egui::Button::new(label).small(),
                                                    )
                                                    .clicked()
                                                {
                                                    install = Some((
                                                        entry.name.clone(),
                                                        entry.repo.clone(),
                                                        overwrite,
                                                    ));
                                                }
                                                ui.end_row();
                                            }
                                        });
                                });
                        }
                    }
                    JinxTab::Repos => {
                        let repos = crate::core::jinx::repo::RepoList::load_or_seed(
                            self.app_core.game_type(),
                        )
                        .map(|l| l.repos)
                        .unwrap_or_default();
                        egui::Grid::new("jinx_repos_grid")
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                for repo in &repos {
                                    ui.strong(&repo.name);
                                    ui.weak(&repo.url);
                                    if ui.small_button("Remove").clicked() {
                                        repo_remove = Some(repo.name.clone());
                                    }
                                    ui.end_row();
                                }
                            });
                        ui.separator();
                        ui.label("Add repository");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.new_repo_name)
                                    .hint_text("name")
                                    .desired_width(120.0),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut state.new_repo_url)
                                    .hint_text("https://…")
                                    .desired_width(240.0),
                            );
                            if ui.button("Add").clicked() {
                                let name = state.new_repo_name.trim().to_string();
                                let url = state.new_repo_url.trim().to_string();
                                if name.is_empty() || url.is_empty() {
                                    state.repo_error = Some("Name and URL are required.".into());
                                } else if !url.starts_with("https://")
                                    && !url.starts_with("http://")
                                {
                                    state.repo_error = Some("URL must be http(s).".into());
                                } else {
                                    repo_add = Some((name, url));
                                }
                            }
                        });
                        if let Some(err) = &state.repo_error {
                            ui.colored_label(ui.visuals().error_fg_color, err);
                        }
                    }
                    JinxTab::Log => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for line in &state.log {
                                    ui.monospace(line);
                                }
                            });
                    }
                }
            });

        // Apply deferred actions after the window closure (they borrow
        // app_core mutably / start worker jobs).
        if refresh {
            state.log.push(self.app_core.jinx_worker.start(Request::Catalog));
        }
        if update_all {
            state
                .log
                .push(self.app_core.jinx_worker.start(Request::AutoUpdate { dry_run: false }));
        }
        if let Some((name, repo, overwrite)) = install {
            state.log.push(self.app_core.jinx_worker.start(Request::Install {
                name,
                only_repo: Some(repo),
                overwrite,
            }));
        }
        if let Some((name, url)) = repo_add {
            match self.jinx_repo_add(&name, &url) {
                Ok(()) => {
                    state.new_repo_name.clear();
                    state.new_repo_url.clear();
                    state.repo_error = None;
                    state.log.push(format!("[jinx] added repo '{name}'"));
                    // New repo may add assets: refresh the catalog.
                    if !self.app_core.jinx_worker.in_flight() {
                        state.log.push(self.app_core.jinx_worker.start(Request::Catalog));
                    }
                }
                Err(e) => state.repo_error = Some(e),
            }
        }
        if let Some(name) = repo_remove {
            match self.jinx_repo_remove(&name) {
                Ok(()) => state.log.push(format!("[jinx] removed repo '{name}'")),
                Err(e) => state.repo_error = Some(e),
            }
        }

        if open {
            self.jinx_panel = Some(state);
        }
    }
}
