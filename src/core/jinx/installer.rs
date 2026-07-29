//! The download primitive: fetch one asset over HTTPS, verify its digest, and
//! land it on disk atomically.
//!
//! This is the generalization of the mapdb downloader (`mapdb_update.rs`): the
//! same hardened shape — a shared `ureq` agent on the native-tls stack, a
//! capped streaming read so a hostile repository can't fill the drive, and a
//! `.part` file swapped into place only after the bytes verify. The mapdb
//! specialization (JSON-array sanity, GitHub releases API) stays in its own
//! module; what lives here is the digest-verified single-file fetch every
//! asset kind shares.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::protocol::{digest_b64, Asset};

/// Hard cap on any single asset download. Skins/layouts are small; game-data
/// XML is a few MB. 64 MB is far above anything legitimate and bounds a
/// hostile or misconfigured repository.
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

/// A shared HTTPS agent on the same native-tls stack as eAccess login and the
/// mapdb downloader — no second TLS stack, no rustls.
pub fn agent() -> Result<ureq::Agent, String> {
    let connector =
        native_tls::TlsConnector::new().map_err(|e| format!("TLS init failed: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(connector))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .user_agent(concat!("vellum-fe/", env!("CARGO_PKG_VERSION")))
        .build())
}

/// Fetch `url` into memory, capped at [`MAX_ASSET_BYTES`]. Small enough to
/// hold the whole asset — we need every byte to verify the digest before
/// anything touches the destination anyway.
pub fn fetch_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let resp = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(404, _) => format!("{url} not found (404)"),
        ureq::Error::Status(code, _) => format!("repository returned {code} for {url}"),
        e => format!("download failed: {e}"),
    })?;
    let mut body = Vec::new();
    resp.into_reader()
        .take(MAX_ASSET_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|e| format!("read failed: {e}"))?;
    if body.len() as u64 > MAX_ASSET_BYTES {
        return Err(format!(
            "asset exceeds {} MB cap",
            MAX_ASSET_BYTES / (1024 * 1024)
        ));
    }
    Ok(body)
}

/// Download `asset` from `repo_url`, verify its digest matches the manifest,
/// and return the verified bytes. Never touches disk — the caller decides
/// where and how the bytes land (a plain write, or an unpack for a bundle).
pub fn download_verified(
    agent: &ureq::Agent,
    repo_url: &str,
    asset: &Asset,
) -> Result<Vec<u8>, String> {
    let url = join_url(repo_url, &asset.file);
    let bytes = fetch_bytes(agent, &url)?;
    let got = digest_b64(&bytes);
    if got != asset.md5 {
        return Err(format!(
            "digest mismatch for {}: manifest says {}, downloaded {}",
            asset.basename(),
            asset.md5,
            got
        ));
    }
    Ok(bytes)
}

/// Write verified bytes to `dest` atomically: a sibling `.part` file, flushed,
/// then renamed over the destination. A crash mid-write leaves the old file
/// intact; only a complete file ever appears at `dest`.
pub fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {} failed: {e}", parent.display()))?;
    }
    let part = part_path(dest);
    let write = || -> Result<(), String> {
        let mut file = std::fs::File::create(&part)
            .map_err(|e| format!("create {} failed: {e}", part.display()))?;
        file.write_all(bytes)
            .map_err(|e| format!("write {} failed: {e}", part.display()))?;
        file.flush()
            .map_err(|e| format!("flush {} failed: {e}", part.display()))?;
        Ok(())
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    // Windows refuses to rename onto an existing file; clear it first.
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("install {} failed: {e}", dest.display())
    })
}

/// `<dest>.part`, the staging path for an atomic write.
fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// Join a repo URL and an asset path without doubling or dropping the slash.
/// Asset `file` fields conventionally start with `/`, but tolerate either.
fn join_url(repo_url: &str, file: &str) -> String {
    let base = repo_url.trim_end_matches('/');
    if file.starts_with('/') {
        format!("{base}{file}")
    } else {
        format!("{base}/{file}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jinx::protocol::Asset;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn asset(file: &str, md5: &str) -> Asset {
        Asset {
            file: file.into(),
            kind: Some("data".into()),
            md5: md5.into(),
            last_commit: 0,
            header: None,
            vellum: None,
        }
    }

    #[test]
    fn url_join_tolerates_slashes() {
        assert_eq!(join_url("https://x/y/", "/a.xml"), "https://x/y/a.xml");
        assert_eq!(join_url("https://x/y", "/a.xml"), "https://x/y/a.xml");
        assert_eq!(join_url("https://x/y", "a.xml"), "https://x/y/a.xml");
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_part() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("sub").join("file.bin");
        write_atomic(&dest, b"hello").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
        // Overwrite works and no .part remains.
        write_atomic(&dest, b"world").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"world");
        assert!(!part_path(&dest).exists());
    }

    /// One-shot HTTP stub: serves a single body at any path. Thread leaks; the
    /// test process exits regardless.
    fn spawn_stub(body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
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

    #[test]
    fn download_verifies_matching_digest() {
        let body = b"<xml>gameobj</xml>".to_vec();
        let base = spawn_stub(body.clone());
        let agent = agent().unwrap();
        let a = asset("/data/gameobj-data.xml", "7qt1tdPIzApVUQB0BpOKxeV3X4w=");
        let got = download_verified(&agent, &base, &a).unwrap();
        assert_eq!(got, body);
    }

    #[test]
    fn download_rejects_digest_mismatch() {
        let base = spawn_stub(b"tampered".to_vec());
        let agent = agent().unwrap();
        let a = asset("/data/gameobj-data.xml", "7qt1tdPIzApVUQB0BpOKxeV3X4w=");
        let err = download_verified(&agent, &base, &a).unwrap_err();
        assert!(err.contains("digest mismatch"), "{err}");
    }
}
