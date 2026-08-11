//! GemStone's own room pictures (`<resource picture='N'/>`).
//!
//! Wrayth shows a picture beside the room name in the story window, fetched
//! from a public, unauthenticated endpoint:
//!
//! ```text
//! https://www.play.net/bfe/gs-art/<id>.jpg
//! ```
//!
//! The wire carries only the NUMBER — never the image, never a URL — so the
//! client resolves the id itself. Art is cached to disk on first sight and
//! never re-fetched.
//!
//! **Off by default, always.** This is outbound traffic to a third party the
//! user did not initiate, so it is an explicit opt-in
//! (`[game_art] enabled`), never a silent default.
//!
//! **A 302 is not an image.** Ids with no art redirect to `/error.asp`,
//! which returns HTML with a 200. Following redirects would cache an error
//! page as a room picture, so redirects are disabled and the body is checked
//! for the JPEG magic bytes before anything is written. A miss is cached as
//! an empty marker file, so a picture-less room does not re-request on every
//! visit.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Where the game's art lives. Public and unauthenticated.
const ART_URL: &str = "https://www.play.net/bfe/gs-art";

/// A room picture is a small banner (id 1 is 192x96, ~29 KB). Anything much
/// larger is not what we asked for.
const MAX_ART_BYTES: u64 = 4 * 1024 * 1024;

/// Ids we have already tried this session, so a failure is not retried on
/// every room entry. Disk caching covers restarts; this covers the session.
fn attempted() -> &'static Mutex<HashSet<u32>> {
    static ATTEMPTED: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
    ATTEMPTED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Cache directory for downloaded game art.
///
/// This is the SAME pool inline images resolve from, not a private folder:
/// downloaded art is referenced by name like any other picture, so it goes
/// through one lookup path instead of a second, path-based one.
pub fn cache_dir() -> Option<PathBuf> {
    crate::config::Config::global_image_category_dir(
        crate::core::inline_image::POOL_CATEGORY,
    )
    .ok()
}

/// Pool name for a downloaded picture.
///
/// Prefixed so Simu's numbering can never collide with a user's own art: a
/// file the user calls `32.png` stays theirs, and this is `gs-art-32`.
pub fn pool_name(id: u32) -> String {
    format!("gs-art-{id}")
}

/// The cached file for `id`, whether or not it exists.
fn cache_path(id: u32) -> Option<PathBuf> {
    cache_dir().map(|dir| dir.join(format!("{}.jpg", pool_name(id))))
}

/// Marker recording that `id` has no art, so a miss is not re-fetched every
/// time the player walks back into the room.
fn miss_path(id: u32) -> Option<PathBuf> {
    cache_dir().map(|dir| dir.join(format!(".{}.missing", pool_name(id))))
}

/// Is this id already resolved on disk (either cached art or a known miss)?
pub fn is_resolved(id: u32) -> bool {
    let cached = cache_path(id).is_some_and(|p| p.exists());
    let missing = miss_path(id).is_some_and(|p| p.exists());
    cached || missing
}

/// The on-disk path for `id`'s art, if we have it.
pub fn cached_art(id: u32) -> Option<PathBuf> {
    cache_path(id).filter(|p| p.exists())
}

/// True when `body` starts with the JPEG magic bytes.
///
/// The endpoint answers a missing id with a redirect to an HTML error page,
/// so "the request succeeded" is not enough — the bytes have to actually be
/// an image or we would cache the error page as a room picture.
pub fn looks_like_jpeg(body: &[u8]) -> bool {
    body.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// Why a fetch failed. The distinction decides what gets remembered:
///
/// - `Missing` — the server answered and the answer is "this id has no art"
///   (4xx, the redirect-to-error-page, a non-JPEG body). Safe to record on disk
///   so the id is never requested again.
/// - `Transient` — anything that could succeed next time (DNS, timeout, TLS,
///   5xx, disk trouble). Must NOT be recorded: a permanent `.missing` marker
///   written during a network blip would silence that room's art forever, with
///   nothing in the UI to clear it.
#[derive(Debug)]
pub enum FetchError {
    Missing(String),
    Transient(String),
}

impl FetchError {
    pub fn reason(&self) -> &str {
        match self {
            Self::Missing(reason) | Self::Transient(reason) => reason,
        }
    }
}

/// Fetch and cache the art for `id`, blocking.
///
/// Returns the cached path on success. Callers run this off the feed thread:
/// a room render must never wait on the network.
pub fn fetch_blocking(id: u32) -> Result<PathBuf, FetchError> {
    use FetchError::{Missing, Transient};

    let Some(dir) = cache_dir() else {
        return Err(Transient("no cache directory".to_string()));
    };
    let Some(path) = cache_path(id) else {
        return Err(Transient("no cache path".to_string()));
    };
    if path.exists() {
        return Ok(path);
    }

    let connector = native_tls::TlsConnector::new()
        .map_err(|e| Transient(format!("TLS init failed: {e}")))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(connector))
        // A missing id redirects to an HTML error page. Following that would
        // hand us a 200 full of markup to cache as an image.
        .redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .user_agent(concat!("vellum-fe/", env!("CARGO_PKG_VERSION")))
        .build();

    let url = format!("{ART_URL}/{id}.jpg");
    let response = agent.get(&url).call().map_err(|e| match e {
        // A 4xx is the server answering "no such art" — definitive. A 5xx is
        // the server failing to answer — retry next session.
        ureq::Error::Status(code, _) if (400..500).contains(&code) => {
            Missing(format!("no art for picture {id} (HTTP {code})"))
        }
        ureq::Error::Status(code, _) => {
            Transient(format!("server error for picture {id} (HTTP {code})"))
        }
        e => Transient(format!("fetch failed: {e}")),
    })?;
    if response.status() != 200 {
        // With redirects disabled, a missing id surfaces as the 302 to the
        // HTML error page — the server's way of saying the art doesn't exist.
        return Err(Missing(format!(
            "no art for picture {id} (HTTP {})",
            response.status()
        )));
    }

    use std::io::Read as _;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_ART_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|e| Transient(format!("read failed: {e}")))?;
    if body.len() as u64 > MAX_ART_BYTES {
        // The asset exists but will never fit the cap — retrying can't help.
        return Err(Missing(format!("picture {id} exceeds the size cap")));
    }
    if !looks_like_jpeg(&body) {
        return Err(Missing(format!(
            "picture {id} is not a JPEG (probably an error page)"
        )));
    }

    std::fs::create_dir_all(&dir).map_err(|e| Transient(format!("cannot create {dir:?}: {e}")))?;
    crate::config::write_atomic(&path, &body)
        .map_err(|e| Transient(format!("cannot write {path:?}: {e}")))?;
    crate::config::pool::invalidate_cache();
    // Re-scan so the just-downloaded picture resolves by name without a
    // restart or a manual .reload.
    crate::core::inline_image::reload();
    Ok(path)
}

/// Record that `id` has no art, so it is not requested again.
pub fn mark_missing(id: u32) {
    let Some(path) = miss_path(id) else {
        return;
    };
    if let Some(dir) = cache_dir() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = crate::config::write_atomic(&path, b"");
}

/// Claim the right to fetch `id` once this session.
///
/// Returns false when the id is already cached, already known-missing, or
/// another attempt has been made — so a flaky network cannot produce a retry
/// storm against play.net.
pub fn claim_fetch(id: u32) -> bool {
    if id == 0 || is_resolved(id) {
        return false;
    }
    let Ok(mut seen) = attempted().lock() else {
        return false;
    };
    seen.insert(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 302 trap: `/error.asp` returns HTML with a 200, so "it
    /// downloaded" is not enough to call it art.
    #[test]
    fn html_error_pages_are_not_accepted_as_art() {
        assert!(!looks_like_jpeg(b"<html><body>Not found</body></html>"));
        assert!(!looks_like_jpeg(b""));
        assert!(!looks_like_jpeg(&[0x89, b'P', b'N', b'G']), "PNG is not JPEG");
        assert!(looks_like_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]));
    }

    /// Picture 0 means "this room has no picture" and must never be fetched.
    #[test]
    fn picture_zero_is_never_fetched() {
        assert!(!claim_fetch(0));
    }

    /// An id is attempted at most once per session, so a flaky network
    /// cannot hammer play.net on every room entry.
    #[test]
    fn an_id_is_only_claimed_once() {
        let id = 987_654;
        attempted().lock().unwrap().remove(&id);
        assert!(claim_fetch(id), "first claim succeeds");
        assert!(!claim_fetch(id), "second claim is refused");
        attempted().lock().unwrap().remove(&id);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// LIVE: hits play.net. Ignored by default so the suite stays offline;
    /// run with `cargo test -- --ignored live_endpoint` to re-verify the
    /// contract this module is built on.
    #[ignore = "network: hits play.net"]
    #[test]
    fn live_endpoint_still_behaves_as_assumed() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        // A real picture downloads and is a JPEG.
        let ok = fetch_blocking(1).expect("picture 1 should exist");
        let bytes = std::fs::read(&ok).expect("cached file readable");
        assert!(looks_like_jpeg(&bytes), "cached bytes must be a JPEG");

        // An id with no art redirects to an HTML error page. It must FAIL
        // rather than caching markup as a picture — and as a MISSING failure,
        // the kind the caller is allowed to remember on disk. A transient
        // network failure here would abort the test rather than mislabel.
        let err = fetch_blocking(32).expect_err("picture 32 has no art");
        assert!(
            matches!(err, FetchError::Missing(_)),
            "an error-page redirect is a definitive miss: {err:?}"
        );
        assert!(
            !cache_path(32).is_some_and(|p| p.exists()),
            "nothing cached: {err:?}"
        );

        std::env::remove_var("VELLUM_FE_DIR");
    }
}
