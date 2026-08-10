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
        /// Asset category the name is scoped to (`compass`, `hands`,
        /// `skin`, …), from `.jinx install <category> <name>`. Names
        /// collide across categories — a `stealthblue` compass set and a
        /// `stealthblue.vellumpack` skin both exist — so the category is
        /// part of naming an asset, not a tiebreaker. `None` means the
        /// user gave a bare name; resolution demands one if it's ambiguous.
        category: Option<String>,
        only_repo: Option<String>,
        overwrite: bool,
    },
    /// Check every tracked asset and update the ones that changed.
    AutoUpdate { dry_run: bool },
    /// Structured catalog of every installable asset across all repos, for the
    /// GUI panel. Delivers one `Effect::Catalog` when done (plus status lines).
    Catalog,
}

/// One installable asset, as the GUI panel needs it: identity + install state.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub name: String,
    pub kind: String,
    pub repo: String,
    /// What to install to get this entry: the set name for set members
    /// (installing one piece means installing its set), else the file name.
    /// Paired with `category` so a name shared across categories still
    /// resolves to exactly this asset.
    pub install_name: String,
    /// Category qualifier for `install_name` (`compass`, `hands`, `skin`).
    pub category: String,
    pub title: Option<String>,
    pub version: Option<String>,
    /// Manifest digest (md5 field); compared against the installed record to
    /// tell whether an update is available.
    pub digest: String,
    /// Repo-side last-commit epoch (0 when the manifest omits it).
    pub last_commit: i64,
    pub installed: bool,
    pub update_available: bool,
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
    /// The GUI panel's asset catalog (all repos), delivered by `Catalog`.
    Catalog(Vec<CatalogEntry>),
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
        Request::Catalog => "[jinx] loading catalog…".to_string(),
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
                        // Set members collapse to one line: a compass is a
                        // dozen files but a single thing to install, and
                        // listing every piece buried the actual choices.
                        let mut sets: Vec<(String, String, usize)> = Vec::new();
                        for asset in installable {
                            any = true;
                            let Some(set) = asset.set_name() else {
                                send(format!("  {} ({})", asset.basename(), asset.kind()), None);
                                continue;
                            };
                            match sets.iter_mut().find(|(name, _, _)| name == set) {
                                Some((_, _, count)) => *count += 1,
                                None => sets.push((set.to_string(), asset.kind().to_string(), 1)),
                            }
                        }
                        for (set, kind, count) in sets {
                            send(
                                format!("  {set} ({kind} set, {count} file{})",
                                    if count == 1 { "" } else { "s" }),
                                None,
                            );
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

        Request::Install { name, category, only_repo, overwrite } => {
            run_install(
                &agent,
                &repos,
                &mut cache,
                &name,
                category.as_deref(),
                only_repo.as_deref(),
                overwrite,
                &send,
            );
        }

        Request::AutoUpdate { dry_run } => {
            run_auto_update(&agent, &repos, &mut cache, dry_run, &send);
        }

        Request::Catalog => {
            let db = InstalledDb::load().unwrap_or_default();
            let mut entries: Vec<CatalogEntry> = Vec::new();
            let mut failed = Vec::new();
            for repo in &repos.repos {
                match cache.get(&agent, repo) {
                    Ok(manifest) => {
                        for asset in &manifest.available {
                            if !asset.is_installable() {
                                continue;
                            }
                            let key = installer::tracking_key(asset);
                            let installed = db.get(&key);
                            // A set member's install unit is its set, not the
                            // single piece; the category qualifies a name that
                            // other categories may also use. Both go through
                            // resolve::install_category so the panel's button
                            // types the same word the command line accepts.
                            let install_name = match asset.set_name() {
                                Some(set) => set.to_string(),
                                None => asset.basename().to_string(),
                            };
                            let category = resolve::install_category(asset);
                            // Set pieces show as "<set>/<role>" so the
                            // catalog can't show forty identical rows.
                            entries.push(CatalogEntry {
                                name: key.clone(),
                                kind: asset.kind().to_string(),
                                repo: repo.name.clone(),
                                install_name,
                                category,
                                title: asset.vellum.as_ref().and_then(|v| v.title.clone()),
                                version: asset.vellum.as_ref().and_then(|v| v.version.clone()),
                                digest: asset.md5.clone(),
                                last_commit: asset.last_commit,
                                installed: installed.is_some(),
                                update_available: installed
                                    .is_some_and(|rec| rec.digest != asset.md5),
                            });
                        }
                    }
                    Err(e) => failed.push(format!("{} unavailable: {e}", repo.name)),
                }
            }
            entries.sort_by(|a, b| {
                a.kind
                    .cmp(&b.kind)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            for line in failed {
                send(format!("[jinx] {line}"), None);
            }
            let count = entries.len();
            send(
                format!("[jinx] catalog: {count} asset{}", if count == 1 { "" } else { "s" }),
                Some(Effect::Catalog(entries)),
            );
        }
    }
}

/// Install/update one asset and persist the metadata db.
#[allow(clippy::too_many_arguments)]
fn run_install(
    agent: &ureq::Agent,
    repos: &RepoList,
    cache: &mut ManifestCache,
    name: &str,
    category: Option<&str>,
    only_repo: Option<&str>,
    overwrite: bool,
    send: &dyn Fn(String, Option<Effect>),
) {
    // One file, or every member of a set — a set is only usable complete, so
    // it installs as a unit.
    let matches = match resolve::resolve_target(agent, repos, cache, name, category, only_repo) {
        Ok(m) => m,
        Err(e) => return send(format!("[jinx] {e}"), None),
    };
    let is_set = matches.len() > 1 || matches.first().is_some_and(|m| m.asset.set_name().is_some());

    let mut db = InstalledDb::load().unwrap_or_default();
    let (mut installed, mut current, mut failed) = (0usize, 0usize, 0usize);
    let mut kind = String::new();
    for m in &matches {
        match installer::install_asset(agent, &m.repo, &m.asset, &mut db, overwrite) {
            Ok(InstallOutcome::Installed { .. }) => {
                installed += 1;
                kind = m.asset.kind().to_string();
                // Per-file progress would be a dozen lines for one compass;
                // single installs still get their own line below.
                if !is_set {
                    send(format!("[jinx] {} installed", m.asset.basename()), None);
                }
            }
            Ok(InstallOutcome::AlreadyCurrent) => {
                current += 1;
                if !is_set {
                    send(format!("[jinx] {} already up to date", m.asset.basename()), None);
                }
            }
            Err(e) => {
                failed += 1;
                send(format!("[jinx] {e}"), None);
            }
        }
    }
    if let Err(e) = db.save() {
        send(format!("[jinx] installed {name} but tracking save failed: {e}"), None);
    }

    if is_set {
        // A partly-installed set renders with holes, so say so plainly
        // rather than reporting a clean success.
        if failed > 0 {
            send(
                format!("[jinx] {name}: {installed} installed, {failed} failed — set is incomplete"),
                None,
            );
        } else if installed == 0 {
            send(format!("[jinx] {name} already up to date ({current} files)"), None);
        } else {
            send(
                format!("[jinx] {name} installed ({installed} file{}{})",
                    plural(installed),
                    if current > 0 { format!(", {current} already current") } else { String::new() }),
                None,
            );
        }
    }
    // The effect drives cache invalidation and pickers; fire it once for the
    // whole set, named by what the user asked for.
    if installed > 0 {
        send(
            String::new(),
            Some(Effect::Installed { name: name.to_string(), kind }),
        );
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
        // Match on the tracking key, not the basename: set members share
        // basenames across sets, so a basename match could update a piece
        // from the wrong set over this one.
        let Some(asset) = manifest
            .available
            .iter()
            .find(|a| installer::tracking_key(a) == name)
            .cloned()
        else {
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
