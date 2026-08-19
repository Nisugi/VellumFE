//! Fetching and caching repository manifests.
//!
//! `GET {repo}/manifest.json` with a couple of retries (transient network and
//! flaky static hosts are common), parsed into [`Manifest`]. A [`ManifestCache`]
//! holds results per repo for the life of one command so listing, searching,
//! and installing don't each re-fetch. Nothing here writes to disk.

use std::collections::HashMap;

use super::installer;
use super::protocol::Manifest;
use super::repo::RepoSource;

/// Fetch one repo's manifest, retrying a few times before giving up. Errors are
/// returned rather than logged-and-swallowed so the caller can report which
/// repo failed (Jinx's `list` skips a bad repo but names it).
pub fn fetch(agent: &ureq::Agent, repo: &RepoSource) -> Result<Manifest, String> {
    fetch_with_attempts(agent, repo, 3)
}

fn fetch_with_attempts(
    agent: &ureq::Agent,
    repo: &RepoSource,
    attempts: usize,
) -> Result<Manifest, String> {
    let url = format!("{}/manifest.json", repo.url.trim_end_matches('/'));
    let mut last_err = String::new();
    for _ in 0..attempts.max(1) {
        match installer::fetch_bytes(agent, &url) {
            Ok(bytes) => match serde_json::from_slice::<Manifest>(&bytes) {
                Ok(manifest) => return Ok(manifest),
                // A parse error won't fix itself on retry — fail fast.
                Err(e) => return Err(format!("{}: bad manifest.json ({e})", repo.name)),
            },
            Err(e) => last_err = e,
        }
    }
    Err(format!("{}: {last_err}", repo.name))
}

/// Per-command manifest cache. Keyed by repo name; a failed fetch is cached as
/// an `Err` so a broken repo is reported once, not retried on every lookup.
#[derive(Default)]
pub struct ManifestCache {
    entries: HashMap<String, Result<Manifest, String>>,
}

impl ManifestCache {
    pub fn new() -> ManifestCache {
        ManifestCache::default()
    }

    /// The repo's manifest, fetching (and caching) on first request.
    pub fn get(&mut self, agent: &ureq::Agent, repo: &RepoSource) -> &Result<Manifest, String> {
        self.entries
            .entry(repo.name.clone())
            .or_insert_with(|| fetch(agent, repo))
    }

    /// Drop a cached entry so the next `get` re-fetches (e.g. after an install
    /// that may have changed availability).
    pub fn invalidate(&mut self, repo_name: &str) {
        self.entries.remove(repo_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jinx::installer::agent;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Serves `body` at every path, counting requests so we can prove caching.
    fn spawn_counting_stub(body: Vec<u8>, count: Arc<AtomicUsize>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                count.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut stream = reader.into_inner();
                let mut resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                resp.extend_from_slice(&body);
                let _ = stream.write_all(&resp);
            }
        });
        base
    }

    fn repo(name: &str, url: &str) -> RepoSource {
        RepoSource {
            name: name.into(),
            url: url.into(),
        }
    }

    #[test]
    fn fetches_and_parses_manifest() {
        let body = br#"{"available":[{"file":"/go2.lic","md5":"a="}]}"#.to_vec();
        let base = spawn_counting_stub(body, Arc::new(AtomicUsize::new(0)));
        let m = fetch(&agent().unwrap(), &repo("r", &base)).unwrap();
        assert_eq!(m.available.len(), 1);
        assert_eq!(m.available[0].basename(), "go2.lic");
    }

    #[test]
    fn cache_serves_second_lookup_without_refetch() {
        let body = br#"{"available":[]}"#.to_vec();
        let count = Arc::new(AtomicUsize::new(0));
        let base = spawn_counting_stub(body, count.clone());
        let ag = agent().unwrap();
        let r = repo("r", &base);
        let mut cache = ManifestCache::new();

        assert!(cache.get(&ag, &r).is_ok());
        assert!(cache.get(&ag, &r).is_ok());
        assert_eq!(count.load(Ordering::SeqCst), 1, "second get must hit cache");

        cache.invalidate("r");
        assert!(cache.get(&ag, &r).is_ok());
        assert_eq!(count.load(Ordering::SeqCst), 2, "invalidate forces refetch");
    }
}
