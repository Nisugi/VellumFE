//! Finding an asset by name across the configured repositories.
//!
//! A user names an asset (`gameobj-data.xml`, `parchment`); resolution figures
//! out which repo advertises it and which manifest entry it is. Mirrors Jinx's
//! `ensure_specific`: exactly one match installs; zero or many is an error the
//! caller surfaces (with a `--repo` hint for the ambiguous case).

use super::manifest::ManifestCache;
use super::protocol::Asset;
use super::repo::{RepoList, RepoSource};

/// One asset found in one repo.
pub struct Match {
    pub repo: RepoSource,
    pub asset: Asset,
}

/// Every (repo, asset) whose basename or stem matches `name`, across all repos.
/// A repo whose manifest fails to load is skipped (its error stays in the
/// cache for the caller to report).
///
/// Names are matched as typed: `asset_matches` handles both an exact basename
/// (`gameobj-data.xml`) and a dotless stem (`parchment` → `parchment.vellumpack`),
/// so no `.lic`-style extension guessing is needed here.
pub fn find_all(
    agent: &ureq::Agent,
    repos: &RepoList,
    cache: &mut ManifestCache,
    name: &str,
    only_repo: Option<&str>,
) -> Vec<Match> {
    let mut matches = Vec::new();
    for repo in &repos.repos {
        if only_repo.is_some_and(|r| r != repo.name) {
            continue;
        }
        let Ok(manifest) = cache.get(agent, repo) else {
            continue;
        };
        for asset in &manifest.available {
            if asset_matches(asset, name) {
                matches.push(Match {
                    repo: repo.clone(),
                    asset: asset.clone(),
                });
            }
        }
    }
    matches
}

/// An asset matches when its basename equals the wanted name, or (for dotless
/// input) its basename *stem* does — so `parchment` matches
/// `parchment.vellumpack` and `gameobj-data.xml` matches exactly.
fn asset_matches(asset: &Asset, wanted: &str) -> bool {
    let base = asset.basename();
    if base == wanted {
        return true;
    }
    if !wanted.contains('.') {
        if let Some(stem) = base.rsplit_once('.').map(|(s, _)| s) {
            return stem == wanted;
        }
    }
    false
}

/// Resolve to exactly one match, or an error explaining why not — the gate
/// before any install. `--repo` disambiguates when several repos carry the
/// name.
pub fn resolve_one(
    agent: &ureq::Agent,
    repos: &RepoList,
    cache: &mut ManifestCache,
    name: &str,
    only_repo: Option<&str>,
) -> Result<Match, String> {
    let mut matches = find_all(agent, repos, cache, name, only_repo);
    match matches.len() {
        1 => Ok(matches.pop().unwrap()),
        0 => Err(format!(
            "no repository advertises '{name}'. Try `.jinx search {name}` or `.jinx list`."
        )),
        _ => {
            let where_from = matches
                .iter()
                .map(|m| format!("  - {} ({})", m.repo.name, m.asset.kind()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!(
                "'{name}' is in more than one repository; add --repo=<name>:\n{where_from}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jinx::protocol::Asset;

    fn asset(file: &str, kind: &str) -> Asset {
        Asset {
            file: file.into(),
            kind: Some(kind.into()),
            md5: "x".into(),
            last_commit: 0,
            header: None,
            vellum: None,
        }
    }

    #[test]
    fn asset_matches_exact_and_stem() {
        let skin = asset("/skins/parchment.vellumpack", "skin");
        assert!(asset_matches(&skin, "parchment.vellumpack")); // exact
        assert!(asset_matches(&skin, "parchment")); // stem, dotless input
        assert!(!asset_matches(&skin, "parch")); // partial stem: no

        let data = asset("/data/gameobj-data.xml", "data");
        assert!(asset_matches(&data, "gameobj-data.xml"));
        // A dotted query must match the full basename, not a stem.
        assert!(!asset_matches(&data, "gameobj-data.json"));
    }
}
