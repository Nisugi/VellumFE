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

use crate::config::Config;

use super::metadata::{InstalledAsset, InstalledDb};
use super::protocol::{digest_b64, Asset};
use super::repo::RepoSource;

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

/// What an install did, for user-facing reporting.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Freshly installed or updated to `path`.
    Installed { path: PathBuf },
    /// Already present with the same digest; nothing changed.
    AlreadyCurrent,
}

/// Install one resolved asset: download, verify, and land it by kind. Records
/// the install in `db` (the caller persists `db` afterward). `overwrite`
/// governs replacing an existing, differing local copy.
///
/// J1 implements the **plain-file** kinds fully (`data`, `iconmap`, `image`);
/// composed `skin`/`layout` bundle extraction lands next and returns a clear
/// error until then. `script`/`engine` are refused outright — not VellumFE's
/// domain.
pub fn install_asset(
    agent: &ureq::Agent,
    repo: &RepoSource,
    asset: &Asset,
    db: &mut InstalledDb,
    overwrite: bool,
) -> Result<InstallOutcome, String> {
    let name = asset.basename().to_string();
    let kind = asset.kind();

    // Refuse code assets up front — no execution path exists or should.
    if matches!(kind, "script" | "engine") {
        return Err(format!(
            "'{name}' is a {kind}; VellumFE installs data and interface assets only \
             (scripts stay in Lich)"
        ));
    }
    // Refuse anything VellumFE has no home for: map tiles (the map subsystem's
    // job) and Lich-only data files (sloot.ui, lockpicks.yaml, …). The same
    // gate list/search use, so what installs is exactly what's listed.
    if !asset.is_installable() {
        if kind == "data" {
            return Err(format!(
                "'{name}' is a data file VellumFE doesn't use; \
                 game data is gameobj-data.xml / effect-list.xml"
            ));
        }
        return Err(format!(
            "'{name}' ({kind}) isn't a VellumFE-installable asset \
             (maps are managed with .mapdb)"
        ));
    }

    let dest = plain_file_dest(asset)?;

    if let Some(dest) = &dest {
        // Idempotence: an on-disk file whose digest already matches is a no-op,
        // regardless of overwrite.
        if dest.is_file() {
            if let Ok(existing) = std::fs::read(dest) {
                if digest_b64(&existing) == asset.md5 {
                    record(db, &name, repo, asset);
                    return Ok(InstallOutcome::AlreadyCurrent);
                }
            }
            if !overwrite {
                return Err(format!(
                    "{} already exists and differs; re-run with --force to overwrite",
                    dest.display()
                ));
            }
        }
    }

    let bytes = download_verified(agent, &repo.url, asset)?;

    match dest {
        Some(dest) => {
            write_atomic(&dest, &bytes)?;
            record(db, &name, repo, asset);
            Ok(InstallOutcome::Installed { path: dest })
        }
        // Composed bundles are extracted from the verified zip, not written
        // whole (§3A: a skin/layout is a directory).
        None => {
            let path = install_bundle(kind, &name, &bytes)?;
            record(db, &name, repo, asset);
            Ok(InstallOutcome::Installed { path })
        }
    }
}

/// Extract a verified bundle zip by kind. Skins get a jinx-owned whitelisted
/// extraction into `skins/<name>/`; layouts/UI-packs go through the existing
/// whitelisted, backed-up `uipack::apply`. Returns a representative path.
fn install_bundle(kind: &str, name: &str, zip_bytes: &[u8]) -> Result<PathBuf, String> {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    match kind {
        "skin" => {
            let skin_name = super::bundle::install_skin(stem, zip_bytes)?;
            Config::skins_dir()
                .map(|d| d.join(skin_name))
                .map_err(|e| format!("cannot resolve skins dir: {e}"))
        }
        "layout" | "uipack" => {
            // uipack::apply reads a file on disk; stage the verified zip in a
            // temp file, apply, then drop it.
            let base = Config::base_dir().map_err(|e| format!("cannot resolve base dir: {e}"))?;
            let tmp = base.join(format!(".jinx-{stem}.vellumpack.part"));
            write_atomic(&tmp, zip_bytes)?;
            let result = crate::core::uipack::apply(&base, &tmp, None)
                .map_err(|e| format!("applying '{name}': {e:#}"));
            let _ = std::fs::remove_file(&tmp);
            result?;
            // The pack landed via uipack::apply (which owns its layered
            // destinations); return the config base as a representative path.
            Ok(base.join("layouts"))
        }
        other => Err(format!("no bundle installer for kind '{other}'")),
    }
}

/// Destination path for a plain single-file asset, or `None` for a kind that is
/// a composed bundle needing extraction (`skin`, `layout`, `uipack`).
fn plain_file_dest(asset: &Asset) -> Result<Option<PathBuf>, String> {
    let name = asset.basename();
    let kind = asset.kind();
    // mapdb.json is `data`-typed but belongs in the map dir the map subsystem
    // reads, not the game-data store. The mapdb downloader versions its own
    // filenames; a direct .jinx install lands the plain name there and J3's
    // reload hands it to the map service.
    if name.eq_ignore_ascii_case("mapdb.json") {
        let base = Config::base_dir().map_err(|e| format!("cannot resolve base dir: {e}"))?;
        return Ok(Some(
            crate::core::mapdb_update::download_dir(&base).join(name),
        ));
    }
    let dir = match kind {
        // Game data resolves through the data-pack local-store tier.
        "data" => Config::global_data_dir(),
        // Individual icon maps / images drop into the shared icon pool next to
        // the ones already there (config/skins.rs load_global_sheets reads it).
        "iconmap" | "image" | "icon" => Config::global_icons_dir(),
        // Standalone injury-doll base images drop into the doll pool; a skin's
        // [injury_doll] base references one by (absolute) path.
        "doll" => Config::global_dolls_dir(),
        // Established per-file pool categories keep their fixed mapping;
        // skins (and the per-window frame picker's [frames.*] entries)
        // reference them by pool-relative path ("frames/iron.png").
        "frame" => Config::global_image_category_dir("frames"),
        "background" => Config::global_image_category_dir("backgrounds"),
        "compass" => Config::global_image_category_dir("compass"),
        "statusicon" => Config::global_image_category_dir("statusicons"),
        // Composed bundles: extracted elsewhere, not a plain write.
        "skin" | "layout" | "uipack" => return Ok(None),
        // Anything else installs into the pool folder the manifest names
        // (sanitized by pool_category), so new categories need no client
        // change. is_installable already gated on the pool being present.
        other => match asset.pool_category() {
            Some(pool) => Config::global_image_category_dir(pool),
            None => return Err(format!("unknown asset kind '{other}' for '{name}'")),
        },
    };
    let dir = dir.map_err(|e| format!("cannot resolve install dir: {e}"))?;
    Ok(Some(dir.join(name)))
}

fn record(db: &mut InstalledDb, name: &str, repo: &RepoSource, asset: &Asset) {
    db.record(
        name,
        InstalledAsset {
            repo: repo.name.clone(),
            digest: asset.md5.clone(),
            version: asset.vellum.as_ref().and_then(|v| v.version.clone()),
            kind: asset.kind().to_string(),
        },
    );
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

    // --- install_asset dispatch (env-dependent: serialize VELLUM_FE_DIR) ---

    use crate::config::VELLUM_FE_DIR_TEST_LOCK as ENV_LOCK;
    use crate::core::jinx::metadata::InstalledDb;
    use crate::core::jinx::repo::RepoSource;

    fn repo(url: &str) -> RepoSource {
        RepoSource { name: "test-repo".into(), url: url.into() }
    }

    #[test]
    fn install_plain_data_file_then_idempotent_then_update() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());
        // Guard: never write into the real ~/.vellum-fe. This test installs a
        // fake gameobj-data.xml; if VELLUM_FE_DIR weren't in effect it would
        // clobber the user's real data pack and break foreach_tests.
        assert!(
            Config::global_data_dir().unwrap().starts_with(cfg.path()),
            "test config dir must be under the tempdir"
        );

        let ag = agent().unwrap();
        let mut db = InstalledDb::default();

        // Round 1: fresh install of a plain data file into global/data/.
        let body_v1 = b"<xml>gameobj</xml>".to_vec();
        let base = spawn_stub(body_v1.clone());
        let a1 = asset("/data/gameobj-data.xml", "7qt1tdPIzApVUQB0BpOKxeV3X4w=");
        let out = install_asset(&ag, &repo(&base), &a1, &mut db, false).unwrap();
        let dest = match out {
            InstallOutcome::Installed { path } => path,
            other => panic!("expected Installed, got {other:?}"),
        };
        assert_eq!(std::fs::read(&dest).unwrap(), body_v1);
        assert!(dest.ends_with("global/data/gameobj-data.xml") ||
                dest.ends_with("global\\data\\gameobj-data.xml"));
        // Metadata recorded with the delivered digest.
        assert_eq!(db.get("gameobj-data.xml").unwrap().digest, a1.md5);
        assert_eq!(db.get("gameobj-data.xml").unwrap().kind, "data");

        // Round 2: same digest is a no-op even without --force.
        let base = spawn_stub(body_v1.clone());
        let out = install_asset(&ag, &repo(&base), &a1, &mut db, false).unwrap();
        assert_eq!(out, InstallOutcome::AlreadyCurrent);

        // Round 3: a differing remote without overwrite is refused...
        let body_v2 = b"<xml>gameobj v2</xml>".to_vec();
        let a2 = asset("/data/gameobj-data.xml", &digest_b64(&body_v2));
        let base = spawn_stub(body_v2.clone());
        let err = install_asset(&ag, &repo(&base), &a2, &mut db, false).unwrap_err();
        assert!(err.contains("already exists") && err.contains("--force"), "{err}");

        // ...and applied with overwrite.
        let base = spawn_stub(body_v2.clone());
        let out = install_asset(&ag, &repo(&base), &a2, &mut db, true).unwrap();
        assert!(matches!(out, InstallOutcome::Installed { .. }));
        assert_eq!(std::fs::read(&dest).unwrap(), body_v2);
        assert_eq!(db.get("gameobj-data.xml").unwrap().digest, a2.md5);

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn install_iconmap_lands_in_shared_icon_pool() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());

        let ag = agent().unwrap();
        let mut db = InstalledDb::default();
        let body = b"PNGDATA".to_vec();
        let base = spawn_stub(body.clone());
        let mut a = asset("/icons/runes.png", &digest_b64(&body));
        a.kind = Some("iconmap".into());

        let out = install_asset(&ag, &repo(&base), &a, &mut db, false).unwrap();
        let dest = match out {
            InstallOutcome::Installed { path } => path,
            other => panic!("expected Installed, got {other:?}"),
        };
        assert!(dest.ends_with("global/images/icons/runes.png") ||
                dest.ends_with("global\\images\\icons\\runes.png"), "{}", dest.display());
        assert_eq!(db.get("runes.png").unwrap().kind, "iconmap");

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn install_doll_lands_in_doll_pool() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());

        let ag = agent().unwrap();
        let mut db = InstalledDb::default();
        let body = b"DOLLPNG".to_vec();
        let base = spawn_stub(body.clone());
        let mut a = asset("/dolls/soldier.png", &digest_b64(&body));
        a.kind = Some("doll".into());

        let out = install_asset(&ag, &repo(&base), &a, &mut db, false).unwrap();
        let dest = match out {
            InstallOutcome::Installed { path } => path,
            other => panic!("expected Installed, got {other:?}"),
        };
        assert!(dest.ends_with("global/images/dolls/soldier.png") ||
                dest.ends_with("global\\images\\dolls\\soldier.png"), "{}", dest.display());
        assert_eq!(db.get("soldier.png").unwrap().kind, "doll");

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn install_pool_kinds_land_in_their_category_dirs() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());

        let ag = agent().unwrap();
        for (kind, dir) in [
            ("frame", "frames"),
            ("background", "backgrounds"),
            ("compass", "compass"),
            ("statusicon", "statusicons"),
        ] {
            let mut db = InstalledDb::default();
            let body = format!("PNG-{kind}").into_bytes();
            let base = spawn_stub(body.clone());
            let mut a = asset(&format!("/asset-{kind}.png"), &digest_b64(&body));
            a.kind = Some(kind.into());

            let out = install_asset(&ag, &repo(&base), &a, &mut db, false).unwrap();
            let dest = match out {
                InstallOutcome::Installed { path } => path,
                other => panic!("expected Installed for {kind}, got {other:?}"),
            };
            let unix = format!("global/images/{dir}/asset-{kind}.png");
            let win = unix.replace('/', "\\");
            assert!(
                dest.ends_with(&unix) || dest.ends_with(&win),
                "{kind}: {}",
                dest.display()
            );
            assert_eq!(db.get(&format!("asset-{kind}.png")).unwrap().kind, kind);
        }

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn install_unknown_kind_with_pool_tag_lands_in_named_pool() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());

        let ag = agent().unwrap();
        let mut db = InstalledDb::default();
        let body = b"BANNERPNG".to_vec();
        let base = spawn_stub(body.clone());
        let mut a = asset("/parade.png", &digest_b64(&body));
        a.kind = Some("banner".into());
        a.vellum = Some(crate::core::jinx::protocol::VellumMeta {
            pool: Some("banners".into()),
            ..Default::default()
        });

        let out = install_asset(&ag, &repo(&base), &a, &mut db, false).unwrap();
        let dest = match out {
            InstallOutcome::Installed { path } => path,
            other => panic!("expected Installed, got {other:?}"),
        };
        assert!(
            dest.ends_with("global/images/banners/parade.png")
                || dest.ends_with("global\\images\\banners\\parade.png"),
            "{}",
            dest.display()
        );

        // Same kind without the pool tag: refused (not installable).
        let mut bare = asset("/parade2.png", &digest_b64(&body));
        bare.kind = Some("banner".into());
        assert!(install_asset(&ag, &repo(&base), &bare, &mut db, false).is_err());

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn refuses_script_and_extracts_skin_bundle() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", cfg.path());
        let ag = agent().unwrap();
        let mut db = InstalledDb::default();

        // A script asset is refused outright — no download attempted.
        let mut script = asset("/go2.lic", "z=");
        script.kind = Some("script".into());
        let err = install_asset(&ag, &repo("http://unused"), &script, &mut db, false).unwrap_err();
        assert!(err.contains("scripts stay in Lich"), "{err}");

        // A composed skin now extracts. Build a real (verified) skin zip.
        let zip = {
            use std::io::Write as _;
            let mut buf = Vec::new();
            {
                let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
                w.start_file("skin.toml", zip::write::SimpleFileOptions::default())
                    .unwrap();
                w.write_all(b"[meta]\nname=\"P\"\n").unwrap();
                w.finish().unwrap();
            }
            buf
        };
        let base = spawn_stub(zip.clone());
        let mut skin = asset("/skins/parchment.vellumpack", &digest_b64(&zip));
        skin.kind = Some("skin".into());
        let out = install_asset(&ag, &repo(&base), &skin, &mut db, false).unwrap();
        assert!(matches!(out, InstallOutcome::Installed { .. }));
        assert!(crate::config::Config::skins_dir()
            .unwrap()
            .join("parchment/skin.toml")
            .is_file());
        assert_eq!(db.get("parchment.vellumpack").unwrap().kind, "skin");

        std::env::remove_var("VELLUM_FE_DIR");
    }
}
