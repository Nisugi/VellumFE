//! Shared game-data assets with three-tier source resolution.
//!
//! Assets like `gameobj-data.xml` are the same files Lich maintains via
//! `;repo`, so freshness comes for free when a Lich install is around.
//! Resolution order per asset:
//!
//! 1. **Lich data folder** (`<lich_dir>/data/<name>`) — freshest; the
//!    user's `;repo` keeps it current. Uses the same `map.lich_dir`
//!    setting the map subsystem reads.
//! 2. **Local store** (`~/.vellum-fe/global/data/<name>`) — a copy the
//!    user dropped in (or a future `.data update` downloaded).
//! 3. **Bundled** (`defaults/globals/<name>`, via `include_str`) — never
//!    fails, refreshed at release time like the mapdb and effect-list.
//!
//! `.data status` reports which tier each asset resolved to and how old
//! the file is; `.data reload` re-resolves mid-session (e.g. after
//! running `;repo download` in Lich). A Settings panel surface is owed
//! (dot-commands are the v1 interface by agreement, 2026-07-28).

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::SystemTime;

/// One registered asset. `lich_rel` is relative to the Lich install dir;
/// these files are game-independent (directly in `data/`, unlike mapdb).
pub struct AssetSpec {
    pub name: &'static str,
    pub lich_rel: &'static str,
    pub bundled: &'static str,
}

pub const GAMEOBJ_DATA: AssetSpec = AssetSpec {
    name: "gameobj-data.xml",
    lich_rel: "data/gameobj-data.xml",
    bundled: include_str!("../../defaults/globals/gameobj-data.xml"),
};

/// Registry for `.data status`. effect-list.xml and the mapdb predate
/// this module and still resolve on their own paths; folding them in is
/// planned, not blocking.
pub const ASSETS: &[&AssetSpec] = &[&GAMEOBJ_DATA];

/// Which tier an asset resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetSource {
    Lich,
    LocalStore,
    Bundled,
}

impl AssetSource {
    pub fn label(&self) -> &'static str {
        match self {
            AssetSource::Lich => "Lich folder",
            AssetSource::LocalStore => "local store",
            AssetSource::Bundled => "bundled",
        }
    }
}

pub struct ResolvedAsset {
    pub content: Cow<'static, str>,
    pub source: AssetSource,
    /// On-disk path for the first two tiers; None when bundled.
    pub path: Option<PathBuf>,
    pub modified: Option<SystemTime>,
}

/// The local store directory: `~/.vellum-fe/global/data/`.
pub fn local_store_dir() -> Option<PathBuf> {
    crate::config::Config::global_data_dir().ok()
}

/// Resolve an asset through the tiers. A tier is used only if its file
/// reads cleanly; unreadable files fall through with a warning.
pub fn resolve(spec: &AssetSpec, lich_dir: Option<&str>) -> ResolvedAsset {
    let lich_path = lich_dir
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(spec.lich_rel));
    let store_path = local_store_dir().map(|dir| dir.join(spec.name));

    let candidates = [
        (AssetSource::Lich, lich_path),
        (AssetSource::LocalStore, store_path),
    ];
    for (source, path) in candidates {
        let Some(path) = path else { continue };
        if !path.is_file() {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let modified = std::fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .ok();
                return ResolvedAsset {
                    content: Cow::Owned(content),
                    source,
                    path: Some(path),
                    modified,
                };
            }
            Err(err) => {
                tracing::warn!(
                    "data pack: cannot read {} ({}); falling back",
                    path.display(),
                    err
                );
            }
        }
    }

    ResolvedAsset {
        content: Cow::Borrowed(spec.bundled),
        source: AssetSource::Bundled,
        path: None,
        modified: None,
    }
}

/// Human lines for `.data status`.
pub fn status_lines(lich_dir: Option<&str>) -> Vec<String> {
    ASSETS
        .iter()
        .map(|spec| {
            let resolved = resolve(spec, lich_dir);
            let age = resolved
                .modified
                .and_then(|m| m.elapsed().ok())
                .map(|elapsed| {
                    let days = elapsed.as_secs() / 86_400;
                    if days == 0 {
                        "updated today".to_string()
                    } else {
                        format!("{} day{} old", days, if days == 1 { "" } else { "s" })
                    }
                })
                .unwrap_or_else(|| "release snapshot".to_string());
            match &resolved.path {
                Some(path) => format!(
                    "{}: {} ({}) - {}",
                    spec.name,
                    resolved.source.label(),
                    age,
                    path.display()
                ),
                None => format!("{}: {} ({})", spec.name, resolved.source.label(), age),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fallback_when_no_lich_dir() {
        let resolved = resolve(&GAMEOBJ_DATA, None);
        // No lich dir and (in CI) no local store: bundled tier, and the
        // bundled snapshot is the real file.
        if resolved.source == AssetSource::Bundled {
            assert!(resolved.content.contains("<type name=\"gem\">"));
            assert!(resolved.path.is_none());
        }
    }

    #[test]
    fn blank_lich_dir_is_ignored() {
        let resolved = resolve(&GAMEOBJ_DATA, Some("   "));
        assert_ne!(resolved.source, AssetSource::Lich);
    }
}
