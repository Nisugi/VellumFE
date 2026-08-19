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
#[derive(Debug)]
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

/// Every member of the named art set, across all repos.
///
/// Set art (a compass, a status-icon sheet family) is only usable complete —
/// a compass missing `ne.png` renders a hole — so the set name is the unit a
/// user installs, not the dozen file names underneath it.
///
/// Matching is case-insensitive on the manifest's `set` field. Assets with no
/// `set` never match here, which is what keeps a self-contained frame or
/// sheet out of set handling.
pub fn find_set(
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
            if asset
                .set_name()
                .is_some_and(|set| set.eq_ignore_ascii_case(name))
                && asset.is_installable()
            {
                matches.push(Match {
                    repo: repo.clone(),
                    asset: asset.clone(),
                });
            }
        }
    }
    matches
}

/// One thing a name resolved to: a whole set, or a single file.
struct Candidate {
    /// Category the user would name to select this ("compass", "skin").
    category: String,
    /// How to install it: every member, or the one file.
    matches: Vec<Match>,
    /// Set name, or `None` for a standalone file.
    set: Option<String>,
}

impl Candidate {
    /// How a user would ask for exactly this.
    fn command(&self, name: &str) -> String {
        match &self.set {
            Some(set) => format!(".jinx install {} {set}", self.category),
            None => format!(".jinx install {} {name}", self.category),
        }
    }

    /// One line describing what this is, for the ambiguity report.
    fn describe(&self) -> String {
        match &self.set {
            Some(set) => format!(
                "{set} ({} set, {} file{})",
                self.category,
                self.matches.len(),
                if self.matches.len() == 1 { "" } else { "s" }
            ),
            None => format!(
                "{} ({})",
                self.matches[0].asset.basename(),
                self.matches[0].asset.kind()
            ),
        }
    }
}

/// The category word a user types to name this asset.
///
/// `pool` first, deliberately. The manifest publishes two category-ish keys
/// with different jobs: `category` copies the top-level `type` (a singular
/// kind noun — `hand`, `frame`) for kind dispatch, while `pool` is the
/// *place* — the plural pool folder that matches both the on-disk directory
/// and the URL path segment (`hands/`). They coincide for `compass` only
/// because that word doesn't pluralize.
///
/// Since a user is naming where art lives, `pool` is the right word — and
/// preferring it keeps one spelling per category. Sourcing set art from
/// `pool` (as `collect_candidates` does) while sourcing standalone files
/// from `category` would make `.jinx install hands bone` and
/// `.jinx install hand <file>` both correct in the same breath.
///
/// `category` and `kind` remain as fallbacks for assets outside the image
/// pool (skins, layouts, data), which publish no `pool`.
pub fn install_category(asset: &Asset) -> String {
    asset
        .pool_category()
        .map(str::to_owned)
        .or_else(|| asset.vellum.as_ref().and_then(|v| v.category.clone()))
        .unwrap_or_else(|| asset.kind().to_owned())
}

/// Resolve an install target: either one file, or every member of one set.
///
/// Names are not unique across categories — a `stealthblue` compass set and
/// a `stealthblue.vellumpack` skin both exist, and a `stealthblue` hand set
/// is a matter of time. So the category is part of naming an asset:
/// `.jinx install compass stealthblue`.
///
/// A bare name still works when it's unambiguous (most of the time). When it
/// isn't, this refuses and reports every candidate with the exact command
/// for each — guessing would silently install the wrong thing, which is
/// precisely the failure a shared namespace invites.
pub fn resolve_target(
    agent: &ureq::Agent,
    repos: &RepoList,
    cache: &mut ManifestCache,
    name: &str,
    category: Option<&str>,
    only_repo: Option<&str>,
) -> Result<Vec<Match>, String> {
    let mut candidates = collect_candidates(agent, repos, cache, name, only_repo);

    // A category scopes the search before ambiguity is judged, so
    // `install compass stealthblue` is exact even though the name is shared.
    if let Some(wanted) = category {
        candidates.retain(|c| c.category.eq_ignore_ascii_case(wanted));
        if candidates.is_empty() {
            return Err(format!(
                "no '{wanted}' asset named '{name}'. Try `.jinx search {name}` or `.jinx list`."
            ));
        }
    }

    pick_candidate(candidates, name)
}

/// Reduce candidates to the one install, or an error naming the choices.
/// Split out from the network-touching resolution so it's unit-testable.
fn pick_candidate(mut candidates: Vec<Candidate>, name: &str) -> Result<Vec<Match>, String> {
    match candidates.len() {
        1 => Ok(candidates.pop().unwrap().matches),
        0 => Err(format!(
            "no repository advertises '{name}'. Try `.jinx search {name}` or `.jinx list`."
        )),
        _ => {
            let lines = candidates
                .iter()
                .map(|c| format!("  - {}\n      {}", c.describe(), c.command(name)))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!(
                "'{name}' matches more than one asset — name the category:\n{lines}"
            ))
        }
    }
}

/// Every distinct thing `name` could mean: one candidate per set, plus one
/// per matching standalone file.
fn collect_candidates(
    agent: &ureq::Agent,
    repos: &RepoList,
    cache: &mut ManifestCache,
    name: &str,
    only_repo: Option<&str>,
) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = Vec::new();

    // Sets, grouped by (category, set) — a set spanning two repos would mix
    // two authors' art, so each repo's copy is its own candidate and the
    // user is told to pick with --repo.
    for m in find_set(agent, repos, cache, name, only_repo) {
        let category = install_category(&m.asset);
        let set = m.asset.set_name().unwrap_or(name).to_owned();
        match candidates
            .iter_mut()
            .find(|c| c.set.as_deref() == Some(set.as_str()) && c.category == category)
        {
            Some(existing) => existing.matches.push(m),
            None => candidates.push(Candidate {
                category,
                set: Some(set),
                matches: vec![m],
            }),
        }
    }

    // Standalone files (skins, frames, data) matching the same name.
    for m in find_all(agent, repos, cache, name, only_repo) {
        if m.asset.set_name().is_some() {
            continue; // already represented by its set
        }
        candidates.push(Candidate {
            category: install_category(&m.asset),
            set: None,
            matches: vec![m],
        });
    }

    candidates
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

    fn set_asset(file: &str, kind: &str, pool: &str, set: &str) -> Asset {
        let mut a = asset(file, kind);
        a.vellum = Some(crate::core::jinx::protocol::VellumMeta {
            pool: Some(pool.into()),
            set: Some(set.into()),
            ..Default::default()
        });
        a
    }

    fn candidate(category: &str, set: Option<&str>, assets: Vec<Asset>) -> Candidate {
        Candidate {
            category: category.into(),
            set: set.map(str::to_owned),
            matches: assets
                .into_iter()
                .map(|asset| Match {
                    repo: RepoSource {
                        name: "r".into(),
                        url: "http://x".into(),
                    },
                    asset,
                })
                .collect(),
        }
    }

    /// The live collision that motivated category-scoped names: a
    /// `stealthblue` compass set and a `stealthblue.vellumpack` skin. Guessing
    /// would silently install the wrong one, so this must refuse and say how
    /// to ask for each.
    #[test]
    fn ambiguous_name_refuses_and_names_both_commands() {
        let compass = candidate(
            "compass",
            Some("stealthblue"),
            vec![
                set_asset("/stealthblue/ne.png", "compass", "compass", "stealthblue"),
                set_asset("/stealthblue/rose.png", "compass", "compass", "stealthblue"),
            ],
        );
        let skin = candidate(
            "skin",
            None,
            vec![asset("/skins/stealthblue.vellumpack", "skin")],
        );

        let err = pick_candidate(vec![compass, skin], "stealthblue").unwrap_err();
        assert!(err.contains("matches more than one asset"), "{err}");
        // Both candidates described, each with the command that selects it.
        assert!(err.contains("stealthblue (compass set, 2 files)"), "{err}");
        assert!(err.contains(".jinx install compass stealthblue"), "{err}");
        assert!(err.contains("stealthblue.vellumpack (skin)"), "{err}");
        assert!(err.contains(".jinx install skin stealthblue"), "{err}");
    }

    /// The manifest publishes two category-ish keys: `category` copies the
    /// singular `type` (`hand`) for kind dispatch, `pool` names the plural
    /// pool folder (`hands`). A user types the place, so `pool` wins — and
    /// set art and standalone files must agree, or `.jinx install hands bone`
    /// and `.jinx install hand <file>` would both be correct at once.
    #[test]
    fn install_category_prefers_pool_over_singular_type() {
        // Hands: the two keys disagree (hand vs hands) — pool wins.
        let mut hand = asset("/bone/lefthand.png", "hand");
        hand.vellum = Some(crate::core::jinx::protocol::VellumMeta {
            category: Some("hand".into()),
            pool: Some("hands".into()),
            set: Some("bone".into()),
            ..Default::default()
        });
        assert_eq!(install_category(&hand), "hands");

        // A standalone file in the same pool resolves to the same word.
        let mut loose = asset("/hands/custom.png", "hand");
        loose.vellum = Some(crate::core::jinx::protocol::VellumMeta {
            category: Some("hand".into()),
            pool: Some("hands".into()),
            ..Default::default()
        });
        assert_eq!(install_category(&loose), "hands");

        // Compass: the words coincide, so either source agrees.
        let mut compass = asset("/stormfront/ne.png", "compass");
        compass.vellum = Some(crate::core::jinx::protocol::VellumMeta {
            category: Some("compass".into()),
            pool: Some("compass".into()),
            set: Some("stormfront".into()),
            ..Default::default()
        });
        assert_eq!(install_category(&compass), "compass");

        // Non-pool assets (skins, layouts, data) publish no pool; the
        // fallbacks still give the user a word to type.
        let skin = asset("/skins/stealthblue.vellumpack", "skin");
        assert_eq!(install_category(&skin), "skin");
    }

    /// An unambiguous bare name still installs with no category — the common
    /// case must not get harder.
    #[test]
    fn unambiguous_bare_name_still_resolves() {
        let only = candidate(
            "compass",
            Some("stormfront"),
            vec![set_asset(
                "/stormfront/ne.png",
                "compass",
                "compass",
                "stormfront",
            )],
        );
        let picked = pick_candidate(vec![only], "stormfront").unwrap();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].asset.set_name(), Some("stormfront"));

        // Nothing matched reads as not-found, not as ambiguity.
        let err = pick_candidate(Vec::new(), "nothing").unwrap_err();
        assert!(err.contains("no repository advertises"), "{err}");
    }

    /// Set membership is the manifest's `set` field, matched
    /// case-insensitively — never guessed from the file name.
    #[test]
    fn set_name_gates_membership_and_sanitizes() {
        let mut member = asset("/compass/ne.png", "compass");
        member.vellum = Some(crate::core::jinx::protocol::VellumMeta {
            set: Some("stormfront".into()),
            ..Default::default()
        });
        assert_eq!(member.set_name(), Some("stormfront"));

        // Self-contained art carries no set and never joins one.
        let frame = asset("/frames/iron.png", "frame");
        assert_eq!(frame.set_name(), None);

        // A set that isn't directory-safe reads as no set at all.
        let mut evil = asset("/compass/ne.png", "compass");
        evil.vellum = Some(crate::core::jinx::protocol::VellumMeta {
            set: Some("../escape".into()),
            ..Default::default()
        });
        assert_eq!(evil.set_name(), None);
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
