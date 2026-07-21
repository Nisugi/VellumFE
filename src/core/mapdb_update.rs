//! Downloads the released mapdb from a GitHub repository — the map without
//! a Lich install (the point on mobile, where there is no Lich folder).
//!
//! Releases come from the Cartographer pipeline: each carries a `mapdb.json`
//! asset in the same Lich format `MapDb::load` already parses. Files land in
//! `<base>/mapdb/` as `mapdb-<tag>.json`, versioned side-by-side; the
//! previous version is kept for rollback, older ones are pruned.
//!
//! Follows the `MapService` pattern: an explicit user action spawns a worker
//! thread, the frontend polls `status` each frame. Nothing downloads
//! automatically.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const DEFAULT_API_BASE: &str = "https://api.github.com";
/// The release asset the Cartographer pipeline attaches.
const ASSET_NAME: &str = "mapdb.json";
/// Optional community map-overrides asset riding the same release; saved as
/// `overrides-<tag>.json` beside the mapdb and loaded read-only underneath
/// the user's personal overrides.
const OVERRIDES_ASSET_NAME: &str = "overrides.json";
/// Newest plus one rollback version.
const KEEP_VERSIONS: usize = 2;

/// Where downloaded mapdbs live under the config base dir.
pub fn download_dir(base: &Path) -> PathBuf {
    base.join("mapdb")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    Downloading {
        tag: String,
        received: u64,
        /// Asset size when the release reports one.
        total: Option<u64>,
    },
    UpToDate {
        tag: String,
    },
    Updated {
        tag: String,
    },
    Failed(String),
}

/// Newest downloaded mapdb as `(tag, path)`, by version-aware tag order.
pub fn latest_downloaded(dir: &Path) -> Option<(String, PathBuf)> {
    downloaded_versions(dir).pop()
}

/// All downloaded mapdbs, oldest → newest.
fn downloaded_versions(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(tag) = name
            .strip_prefix("mapdb-")
            .and_then(|rest| rest.strip_suffix(".json"))
        else {
            continue;
        };
        found.push((tag.to_owned(), path.clone()));
    }
    found.sort_by(|(a, _), (b, _)| tag_order(a).cmp(&tag_order(b)));
    found
}

/// Version-aware ordering: numeric runs compare numerically (`v0.10.0` beats
/// `v0.9.1`); the raw tag breaks ties.
fn tag_order(tag: &str) -> (Vec<u64>, String) {
    let nums = tag
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    (nums, tag.to_owned())
}

/// Tags become filenames; anything path-hostile flattens to '-'.
fn safe_tag(tag: &str) -> String {
    tag.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Drives one download at a time; owns the on-disk version store.
pub struct MapDbUpdater {
    dir: PathBuf,
    rx: Option<mpsc::Receiver<UpdateStatus>>,
    pub status: UpdateStatus,
    /// Newest downloaded tag, kept current across downloads and removals.
    pub installed: Option<String>,
    /// Terminal status of the last run, consumed once via `take_finished`
    /// so completion can be announced exactly once on any frontend.
    finished: Option<UpdateStatus>,
}

impl MapDbUpdater {
    pub fn new(dir: PathBuf) -> MapDbUpdater {
        let installed = latest_downloaded(&dir).map(|(tag, _)| tag);
        MapDbUpdater {
            dir,
            rx: None,
            status: UpdateStatus::Idle,
            installed,
            finished: None,
        }
    }

    pub fn in_flight(&self) -> bool {
        self.rx.is_some()
    }

    /// Kick off a check-and-download against `owner/repo`. Ignored while one
    /// is already running.
    pub fn start(&mut self, repo: String) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.status = UpdateStatus::Checking;
        let dir = self.dir.clone();
        let _ = std::thread::Builder::new()
            .name("mapdb-update".into())
            .spawn(move || {
                let mut notify = |status: UpdateStatus| {
                    let _ = tx.send(status);
                };
                let outcome = check_and_download(&repo, &dir, DEFAULT_API_BASE, &mut notify);
                let _ = tx.send(match outcome {
                    Ok(status) => status,
                    Err(e) => {
                        tracing::warn!("mapdb update failed: {e}");
                        UpdateStatus::Failed(e)
                    }
                });
            });
    }

    /// Drain worker events. Returns true when a new mapdb was installed this
    /// poll — the caller should re-resolve the map source.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else {
            return false;
        };
        let mut installed_new = false;
        loop {
            match rx.try_recv() {
                Ok(status) => {
                    if let UpdateStatus::Updated { tag } = &status {
                        self.installed = Some(tag.clone());
                        installed_new = true;
                    }
                    if matches!(
                        status,
                        UpdateStatus::Updated { .. }
                            | UpdateStatus::UpToDate { .. }
                            | UpdateStatus::Failed(_)
                    ) {
                        self.finished = Some(status.clone());
                    }
                    self.status = status;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    // A worker that died without a terminal status (panic,
                    // failed spawn) must not leave the UI stuck on a spinner.
                    if matches!(
                        self.status,
                        UpdateStatus::Checking | UpdateStatus::Downloading { .. }
                    ) {
                        let failed =
                            UpdateStatus::Failed("update worker exited unexpectedly".into());
                        self.finished = Some(failed.clone());
                        self.status = failed;
                    }
                    break;
                }
            }
        }
        installed_new
    }

    /// The last run's terminal status, once.
    pub fn take_finished(&mut self) -> Option<UpdateStatus> {
        self.finished.take()
    }

    /// Delete every downloaded version (falling the map source back to the
    /// Lich folder). No-op while a download is running.
    pub fn remove_downloaded(&mut self) {
        if self.rx.is_some() {
            return;
        }
        for (tag, path) in downloaded_versions(&self.dir) {
            let _ = std::fs::remove_file(path.with_file_name(format!("overrides-{tag}.json")));
            let _ = std::fs::remove_file(path);
        }
        self.installed = None;
        self.status = UpdateStatus::Idle;
    }
}

fn agent() -> Result<ureq::Agent, String> {
    let connector =
        native_tls::TlsConnector::new().map_err(|e| format!("TLS init failed: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(connector))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .user_agent(concat!("vellum-fe/", env!("CARGO_PKG_VERSION")))
        .build())
}

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

fn fetch_latest_release(
    agent: &ureq::Agent,
    api_base: &str,
    repo: &str,
) -> Result<Release, String> {
    let url = format!("{api_base}/repos/{repo}/releases/latest");
    let resp = agent
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => format!("{repo} has no releases (or does not exist)"),
            ureq::Error::Status(code, _) => format!("GitHub returned {code} for {repo}"),
            e => format!("release check failed: {e}"),
        })?;
    let mut body = String::new();
    resp.into_reader()
        .take(4 * 1024 * 1024)
        .read_to_string(&mut body)
        .map_err(|e| format!("release check read failed: {e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("release JSON parse failed: {e}"))
}

/// Check the latest release and download its mapdb if it isn't installed
/// yet. `notify` receives progress; the returned status is terminal.
fn check_and_download(
    repo: &str,
    dir: &Path,
    api_base: &str,
    notify: &mut dyn FnMut(UpdateStatus),
) -> Result<UpdateStatus, String> {
    let agent = agent()?;
    let release = fetch_latest_release(&agent, api_base, repo)?;
    let tag = safe_tag(&release.tag_name);
    if latest_downloaded(dir).is_some_and(|(installed, _)| installed == tag) {
        return Ok(UpdateStatus::UpToDate { tag });
    }
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == ASSET_NAME)
        .ok_or_else(|| format!("release {} has no {ASSET_NAME} asset", release.tag_name))?;
    download_asset(&agent, asset, &tag, dir, notify)?;
    // Community overrides are optional and small; a release without them (or
    // a failed fetch) never fails the mapdb install.
    if let Some(overrides_asset) = release
        .assets
        .iter()
        .find(|a| a.name == OVERRIDES_ASSET_NAME)
    {
        let path = dir.join(format!("overrides-{tag}.json"));
        if let Err(e) = download_small(&agent, &overrides_asset.browser_download_url, &path) {
            tracing::warn!("community overrides download failed for {tag}: {e}");
        }
    }
    prune(dir);
    Ok(UpdateStatus::Updated { tag })
}

/// Fetch a small asset straight to disk (no progress reporting), capped so a
/// hostile release can't fill the drive.
fn download_small(agent: &ureq::Agent, url: &str, path: &Path) -> Result<(), String> {
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut body = Vec::new();
    resp.into_reader()
        .take(16 * 1024 * 1024)
        .read_to_end(&mut body)
        .map_err(|e| format!("read failed: {e}"))?;
    std::fs::write(path, body).map_err(|e| format!("write {} failed: {e}", path.display()))
}

/// The community overrides file paired with a mapdb path, when one exists:
/// `overrides-<tag>.json` beside a downloaded `mapdb-<tag>.json`, or a
/// hand-placed `overrides.json` sibling for explicit/Lich-folder files.
pub fn community_overrides_for(db_path: &Path) -> Option<PathBuf> {
    let name = db_path.file_name()?.to_str()?;
    let mut candidates = Vec::new();
    if let Some(tag) = name
        .strip_prefix("mapdb-")
        .and_then(|rest| rest.strip_suffix(".json"))
    {
        candidates.push(db_path.with_file_name(format!("overrides-{tag}.json")));
    }
    candidates.push(db_path.with_file_name("overrides.json"));
    candidates.into_iter().find(|p| p.is_file())
}

fn download_asset(
    agent: &ureq::Agent,
    asset: &Asset,
    tag: &str,
    dir: &Path,
    notify: &mut dyn FnMut(UpdateStatus),
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create {} failed: {e}", dir.display()))?;
    let final_path = dir.join(format!("mapdb-{tag}.json"));
    let part_path = dir.join(format!("mapdb-{tag}.json.part"));

    let resp = agent
        .get(&asset.browser_download_url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .or((asset.size > 0).then_some(asset.size));
    notify(UpdateStatus::Downloading {
        tag: tag.to_owned(),
        received: 0,
        total,
    });

    let mut reader = resp.into_reader();
    let write_part = |reader: &mut dyn Read, notify: &mut dyn FnMut(UpdateStatus)| {
        let mut file = std::fs::File::create(&part_path)
            .map_err(|e| format!("create {} failed: {e}", part_path.display()))?;
        let mut buf = [0u8; 64 * 1024];
        let mut received = 0u64;
        let mut last_note = 0u64;
        let mut first_byte = None;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("download interrupted: {e}"))?;
            if n == 0 {
                break;
            }
            if first_byte.is_none() {
                first_byte = buf[..n].iter().find(|b| !b.is_ascii_whitespace()).copied();
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("write {} failed: {e}", part_path.display()))?;
            received += n as u64;
            if received - last_note >= 1024 * 1024 {
                last_note = received;
                notify(UpdateStatus::Downloading {
                    tag: tag.to_owned(),
                    received,
                    total,
                });
            }
        }
        file.flush()
            .map_err(|e| format!("write {} failed: {e}", part_path.display()))?;
        // Cheap sanity before the swap: the mapdb is a JSON array, and a
        // truncated transfer must not replace a working database.
        // MapDb::load does the real validation.
        if first_byte != Some(b'[') {
            return Err("downloaded file is not a mapdb JSON array".to_string());
        }
        if asset.size > 0 && received != asset.size {
            return Err(format!(
                "download truncated: got {received} of {} bytes",
                asset.size
            ));
        }
        Ok(())
    };
    if let Err(e) = write_part(&mut reader, notify) {
        let _ = std::fs::remove_file(&part_path);
        return Err(e);
    }

    // Windows rename fails onto an existing file (e.g. re-fetching a tag
    // whose earlier download was removed mid-swap).
    let _ = std::fs::remove_file(&final_path);
    std::fs::rename(&part_path, &final_path)
        .map_err(|e| format!("install {} failed: {e}", final_path.display()))?;
    Ok(final_path)
}

fn prune(dir: &Path) {
    let versions = downloaded_versions(dir);
    if versions.len() > KEEP_VERSIONS {
        for (tag, path) in &versions[..versions.len() - KEEP_VERSIONS] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(path.with_file_name(format!("overrides-{tag}.json")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    #[test]
    fn tag_ordering_is_numeric_not_lexicographic() {
        assert!(tag_order("v0.10.0") > tag_order("v0.9.1"));
        assert!(tag_order("v1.0.0") > tag_order("v0.99.99"));
        assert!(tag_order("v0.4.0") == tag_order("v0.4.0"));
    }

    #[test]
    fn tags_are_flattened_to_safe_filenames() {
        assert_eq!(safe_tag("v0.4.0"), "v0.4.0");
        assert_eq!(safe_tag("release/2026-07"), "release-2026-07");
    }

    #[test]
    fn community_overrides_pair_with_their_mapdb_and_prune_together() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mapdb-v0.3.0.json");
        std::fs::write(&db, "[]").unwrap();
        assert_eq!(community_overrides_for(&db), None);

        // The tagged sibling pairs with a downloaded db.
        let tagged = dir.path().join("overrides-v0.3.0.json");
        std::fs::write(&tagged, "{}").unwrap();
        assert_eq!(community_overrides_for(&db), Some(tagged));

        // Explicit/Lich-folder files fall back to a plain sibling.
        let lich_db = dir.path().join("map-1234.json");
        std::fs::write(&lich_db, "[]").unwrap();
        assert_eq!(community_overrides_for(&lich_db), None);
        let plain = dir.path().join("overrides.json");
        std::fs::write(&plain, "{}").unwrap();
        assert_eq!(community_overrides_for(&lich_db), Some(plain));

        // Pruning an old mapdb takes its overrides with it.
        for tag in ["v0.9.0", "v0.10.0"] {
            std::fs::write(dir.path().join(format!("mapdb-{tag}.json")), "[]").unwrap();
        }
        prune(dir.path());
        assert!(!dir.path().join("mapdb-v0.3.0.json").exists());
        assert!(!dir.path().join("overrides-v0.3.0.json").exists());
        assert!(dir.path().join("mapdb-v0.10.0.json").exists());
    }

    #[test]
    fn version_store_finds_newest_and_prunes_to_two() {
        let dir = tempfile::tempdir().unwrap();
        for tag in ["v0.2.0", "v0.10.0", "v0.9.0"] {
            std::fs::write(dir.path().join(format!("mapdb-{tag}.json")), "[]").unwrap();
        }
        // Stray files never count as versions.
        std::fs::write(dir.path().join("mapdb-v0.11.0.json.part"), "[").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();

        let (tag, path) = latest_downloaded(dir.path()).unwrap();
        assert_eq!(tag, "v0.10.0");
        assert!(path.ends_with("mapdb-v0.10.0.json"));

        prune(dir.path());
        let left: Vec<String> = downloaded_versions(dir.path())
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        assert_eq!(left, ["v0.9.0", "v0.10.0"]);
    }

    /// Minimal HTTP stub: serves a release JSON (pointing back at its own
    /// /asset route) at the API path and the asset body at /asset. One
    /// request per connection; the thread leaks, tests exit anyway.
    fn spawn_stub_with_release(tag: &str, asset_body: &[u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let release = format!(
            r#"{{"tag_name":"{tag}","assets":[{{"name":"mapdb.json","browser_download_url":"{base}/asset","size":{}}}]}}"#,
            asset_body.len()
        )
        .into_bytes();
        let routes = vec![("/repos/", release), ("/asset", asset_body.to_vec())];
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    continue;
                }
                let path = request_line.split_whitespace().nth(1).unwrap_or("/");
                let body = routes
                    .iter()
                    .find(|(prefix, _)| path.starts_with(prefix))
                    .map(|(_, body)| body.clone());
                let mut stream = reader.into_inner();
                let response = match body {
                    Some(body) => {
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        r.extend_from_slice(&body);
                        r
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&response);
            }
        });
        base
    }

    #[test]
    fn downloads_then_reports_up_to_date_then_updates_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let mapdb = br#"[{"id": 1}]"#.to_vec();

        // Round 1: fresh download.
        let base = spawn_stub_with_release("v0.4.0", &mapdb);
        let mut seen = Vec::new();
        let outcome = check_and_download("x/y", dir.path(), &base, &mut |s| seen.push(s)).unwrap();
        assert_eq!(
            outcome,
            UpdateStatus::Updated {
                tag: "v0.4.0".into()
            }
        );
        assert!(matches!(seen.first(), Some(UpdateStatus::Downloading { .. })));
        let (tag, path) = latest_downloaded(dir.path()).unwrap();
        assert_eq!(tag, "v0.4.0");
        assert_eq!(std::fs::read(path).unwrap(), mapdb);

        // Round 2: same tag is a no-op.
        let outcome = check_and_download("x/y", dir.path(), &base, &mut |_| {}).unwrap();
        assert_eq!(
            outcome,
            UpdateStatus::UpToDate {
                tag: "v0.4.0".into()
            }
        );

        // Rounds 3-4: newer tags install and pruning keeps two.
        for tag in ["v0.5.0", "v0.6.0"] {
            let base = spawn_stub_with_release(tag, &mapdb);
            check_and_download("x/y", dir.path(), &base, &mut |_| {}).unwrap();
        }
        let left: Vec<String> = downloaded_versions(dir.path())
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        assert_eq!(left, ["v0.5.0", "v0.6.0"]);
    }

    #[test]
    fn truncated_or_non_json_downloads_never_install() {
        let dir = tempfile::tempdir().unwrap();
        // Body is HTML, not a JSON array (e.g. a captive portal or error page).
        let base = spawn_stub_with_release("v1.0.0", b"<html>err</html>");
        let err = check_and_download("x/y", dir.path(), &base, &mut |_| {}).unwrap_err();
        assert!(err.contains("not a mapdb JSON array"), "{err}");
        assert!(latest_downloaded(dir.path()).is_none());
        assert!(!dir.path().join("mapdb-v1.0.0.json.part").exists());
    }
}
