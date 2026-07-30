//! Off-thread execution for the `.jinx` commands.
//!
//! Every command that touches the network (list, search, info, install,
//! update, auto-update) runs on a worker thread so a slow or unreachable
//! repository never blocks the input loop. The worker streams human-readable
//! lines back over an mpsc channel; the frontend drains them each frame via
//! [`JinxWorker::poll`] into the game text — exactly the `MapDbUpdater` shape
//! (`core/mapdb_update.rs`), which the whole app already polls per frame.
//!
//! Only one job runs at a time; a second request while busy is rejected with a
//! line rather than queued, matching how a user issues these interactively.

use std::sync::mpsc;

use crate::config::GameType;

use super::installer::{self, InstallOutcome};
use super::manifest::ManifestCache;
use super::metadata::InstalledDb;
use super::repo::RepoList;
use super::resolve;

/// A queued `.jinx` operation. Repo-list edits are handled inline by the
/// command layer (no network), so they are not represented here.
#[derive(Debug, Clone)]
pub enum Request {
    /// List every asset across all repos (optionally one).
    List { only_repo: Option<String> },
    /// Regex/substring search across asset names.
    Search { pattern: String },
    /// Show details for one named asset.
    Info { name: String, only_repo: Option<String> },
    /// Install (or update, when `overwrite`) one named asset.
    Install {
        name: String,
        only_repo: Option<String>,
        overwrite: bool,
    },
    /// Check every tracked asset and update the ones that changed.
    AutoUpdate { dry_run: bool },
}

/// One line of output plus whether the job is finished. `done` lets the poll
/// clear the in-flight flag and, for installs, trigger post-install reloads
/// (J3) via the returned effect.
pub struct Update {
    pub line: String,
    pub effect: Option<Effect>,
}

/// A side effect the main thread must apply after an install (reloads can't
/// run on the worker — they touch `AppCore`). J3 fills these in; for now an
/// install just reports the kind + name so the command layer can dispatch.
#[derive(Debug, Clone)]
pub enum Effect {
    Installed { name: String, kind: String },
}

/// Drives at most one `.jinx` job at a time; owns the game gate for repo
/// seeding. Lives on `AppCore`; the frontend polls it each frame.
pub struct JinxWorker {
    game: Option<GameType>,
    rx: Option<mpsc::Receiver<Update>>,
}

impl JinxWorker {
    pub fn new(game: Option<GameType>) -> JinxWorker {
        JinxWorker { game, rx: None }
    }

    /// Update the game gate (e.g. once the character's game is known), so repo
    /// seeding picks the right mapdb-backup repo.
    pub fn set_game(&mut self, game: Option<GameType>) {
        self.game = game;
    }

    pub fn in_flight(&self) -> bool {
        self.rx.is_some()
    }

    /// Start a job. Returns an immediate line (rejection when busy, or a
    /// "working…" acknowledgement) for the command layer to show at once.
    pub fn start(&mut self, request: Request) -> String {
        if self.rx.is_some() {
            return "[jinx] busy — one operation at a time".to_string();
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let game = self.game;
        let ack = request_ack(&request);
        let _ = std::thread::Builder::new()
            .name("jinx-worker".into())
            .spawn(move || run_job(game, request, &tx));
        ack
    }

    /// Drain any lines the worker has produced. Returns them for the caller to
    /// print, and clears the in-flight flag when the job finishes. Called once
    /// per frame.
    pub fn poll(&mut self) -> Vec<Update> {
        let Some(rx) = &self.rx else {
            return Vec::new();
        };
        let mut out = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(update) => out.push(update),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Worker finished (or panicked): job is over.
                    self.rx = None;
                    break;
                }
            }
        }
        out
    }
}

fn request_ack(request: &Request) -> String {
    match request {
        Request::List { .. } => "[jinx] listing assets…".to_string(),
        Request::Search { pattern } => format!("[jinx] searching for '{pattern}'…"),
        Request::Info { name, .. } => format!("[jinx] fetching info for {name}…"),
        Request::Install { name, overwrite, .. } => {
            let verb = if *overwrite { "updating" } else { "installing" };
            format!("[jinx] {verb} {name}…")
        }
        Request::AutoUpdate { dry_run } => {
            if *dry_run {
                "[jinx] checking for updates (dry run)…".to_string()
            } else {
                "[jinx] updating all installed assets…".to_string()
            }
        }
    }
}

/// The worker body: load repos + agent, do the work, send lines. All fallible
/// steps report their error as a line rather than panicking.
fn run_job(game: Option<GameType>, request: Request, tx: &mpsc::Sender<Update>) {
    let send = |line: String, effect: Option<Effect>| {
        let _ = tx.send(Update { line, effect });
    };

    let agent = match installer::agent() {
        Ok(a) => a,
        Err(e) => return send(format!("[jinx] {e}"), None),
    };
    let mut repos = match RepoList::load_or_seed(game) {
        Ok(r) => r,
        Err(e) => return send(format!("[jinx] cannot load repos: {e}"), None),
    };
    // Best-effort: pick up category repos the vellum-assets monorepo has
    // grown since this client shipped (add-only; offline is a no-op).
    repos.discover(&agent);
    let mut cache = ManifestCache::new();

    match request {
        Request::List { only_repo } => {
            let mut any = false;
            for repo in &repos.repos {
                if only_repo.as_deref().is_some_and(|r| r != repo.name) {
                    continue;
                }
                match cache.get(&agent, repo) {
                    Ok(manifest) => {
                        if manifest.available.is_empty() {
                            continue;
                        }
                        let installable: Vec<_> =
                            manifest.available.iter().filter(|a| a.is_installable()).collect();
                        if installable.is_empty() {
                            continue;
                        }
                        send(format!("[jinx] {}:", repo.name), None);
                        for asset in installable {
                            any = true;
                            send(format!("  {} ({})", asset.basename(), asset.kind()), None);
                        }
                    }
                    Err(e) => send(format!("[jinx] {} unavailable: {e}", repo.name), None),
                }
            }
            if !any {
                send("[jinx] no installable assets found".to_string(), None);
            }
        }

        Request::Search { pattern } => {
            let needle = pattern.to_lowercase();
            let mut hits = 0;
            for repo in &repos.repos {
                let Ok(manifest) = cache.get(&agent, repo) else { continue };
                for asset in &manifest.available {
                    if !asset.is_installable() {
                        continue;
                    }
                    if asset.basename().to_lowercase().contains(&needle) {
                        hits += 1;
                        send(
                            format!("  {} ({}) — {}", asset.basename(), asset.kind(), repo.name),
                            None,
                        );
                    }
                }
            }
            send(format!("[jinx] {hits} match{}", if hits == 1 { "" } else { "es" }), None);
        }

        Request::Info { name, only_repo } => {
            match resolve::resolve_one(&agent, &repos, &mut cache, &name, only_repo.as_deref()) {
                Ok(m) => {
                    let a = &m.asset;
                    send(format!("[jinx] {} ({}, repo: {})", a.basename(), a.kind(), m.repo.name), None);
                    if let Some(v) = &a.vellum {
                        if let Some(t) = &v.title { send(format!("  {t}"), None); }
                        if let Some(au) = &v.author { send(format!("  by {au}"), None); }
                        if let Some(d) = &v.description { send(format!("  {d}"), None); }
                        if let Some(ver) = &v.version { send(format!("  version {ver}"), None); }
                        if !v.tags.is_empty() { send(format!("  tags: {}", v.tags.join(", ")), None); }
                    }
                }
                Err(e) => send(format!("[jinx] {e}"), None),
            }
        }

        Request::Install { name, only_repo, overwrite } => {
            run_install(&agent, &repos, &mut cache, &name, only_repo.as_deref(), overwrite, &send);
        }

        Request::AutoUpdate { dry_run } => {
            run_auto_update(&agent, &repos, &mut cache, dry_run, &send);
        }
    }
}

/// Install/update one asset and persist the metadata db.
fn run_install(
    agent: &ureq::Agent,
    repos: &RepoList,
    cache: &mut ManifestCache,
    name: &str,
    only_repo: Option<&str>,
    overwrite: bool,
    send: &dyn Fn(String, Option<Effect>),
) {
    let m = match resolve::resolve_one(agent, repos, cache, name, only_repo) {
        Ok(m) => m,
        Err(e) => return send(format!("[jinx] {e}"), None),
    };
    let mut db = InstalledDb::load().unwrap_or_default();
    match installer::install_asset(agent, &m.repo, &m.asset, &mut db, overwrite) {
        Ok(InstallOutcome::Installed { .. }) => {
            if let Err(e) = db.save() {
                send(format!("[jinx] installed {name} but tracking save failed: {e}"), None);
            }
            let kind = m.asset.kind().to_string();
            send(
                format!("[jinx] {} installed", m.asset.basename()),
                Some(Effect::Installed { name: m.asset.basename().to_string(), kind }),
            );
        }
        Ok(InstallOutcome::AlreadyCurrent) => {
            let _ = db.save();
            send(format!("[jinx] {} already up to date", m.asset.basename()), None);
        }
        Err(e) => send(format!("[jinx] {e}"), None),
    }
}

/// Diff every tracked asset against its repo and update the changed ones.
fn run_auto_update(
    agent: &ureq::Agent,
    repos: &RepoList,
    cache: &mut ManifestCache,
    dry_run: bool,
    send: &dyn Fn(String, Option<Effect>),
) {
    let mut db = InstalledDb::load().unwrap_or_default();
    if db.assets.is_empty() {
        return send("[jinx] nothing installed to update".to_string(), None);
    }
    // Snapshot names first; we may mutate db during the loop.
    let tracked: Vec<(String, String)> = db
        .assets
        .iter()
        .map(|(name, rec)| (name.clone(), rec.repo.clone()))
        .collect();

    let mut updated = 0;
    let mut available = 0;
    for (name, repo_name) in tracked {
        let Some(repo) = repos.find(&repo_name).cloned() else {
            send(format!("[jinx] {name}: repo '{repo_name}' no longer configured"), None);
            continue;
        };
        let Ok(manifest) = cache.get(agent, &repo) else { continue };
        let Some(asset) = manifest.available.iter().find(|a| a.basename() == name).cloned() else {
            send(format!("[jinx] {name}: no longer in {repo_name}"), None);
            continue;
        };
        let current = db.get(&name).map(|r| r.digest.clone()).unwrap_or_default();
        if current == asset.md5 {
            continue; // up to date
        }
        available += 1;
        if dry_run {
            send(format!("  update available: {name}"), None);
            continue;
        }
        match installer::install_asset(agent, &repo, &asset, &mut db, true) {
            Ok(_) => {
                updated += 1;
                send(format!("  updated {name}"), None);
            }
            Err(e) => send(format!("  {name}: {e}"), None),
        }
    }

    if dry_run {
        send(format!("[jinx] {available} update{} available", plural(available)), None);
    } else {
        let _ = db.save();
        send(format!("[jinx] updated {updated} asset{}", plural(updated)), None);
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
