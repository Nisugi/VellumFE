//! The repository source list: which hosts VellumFE fetches manifests from.
//!
//! Stored as `~/.vellum-fe/repos.toml`, a list of `[[repo]]` entries. On first
//! run the file is seeded with the standard sources — the existing
//! elanthia-online game-data repos plus VellumFE's own asset repos — gated by
//! game (GS vs DR) exactly as Jinx's `Setup.apply` does. Users manage the list
//! through `.jinx repo` commands, never by hand-editing the TOML (the project's
//! no-TOML-hand-editing rule).

use serde::{Deserialize, Serialize};

use crate::config::{Config, GameType};

/// One repository: a display name and a static host URL. Assets are fetched as
/// `{url}/manifest.json` and `{url}{asset.file}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSource {
    pub name: String,
    pub url: String,
}

/// The `repos.toml` document: a flat list of sources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoList {
    #[serde(default, rename = "repo")]
    pub repos: Vec<RepoSource>,
}

/// A seed entry with the game it applies to (`None` = every game).
struct Seed {
    name: &'static str,
    url: &'static str,
    only: Option<GameType>,
}

/// Standard sources, seeded on first run. Game-data repos are the existing
/// elanthia-online infrastructure (reused as-is); the `vellum-*` repos are our
/// own single monorepo's per-category sub-paths.
const SEEDS: &[Seed] = &[
    // Game data + community assets, all games.
    Seed {
        name: "elanthia-online",
        url: "https://extras.repo.elanthia.online",
        only: None,
    },
    // Map backups, game-specific.
    Seed {
        name: "mapdb-backup-gs",
        url: "https://elanthia-online.github.io/mapdb-backup-gs",
        only: Some(GameType::GS4),
    },
    Seed {
        name: "mapdb-backup-dr",
        url: "https://elanthia-online.github.io/mapdb-backup-dr",
        only: Some(GameType::DR),
    },
    // VellumFE asset repos: per-category sub-paths of the vellum-assets
    // monorepo, served over GitHub Pages (live). Each category is its own
    // Jinx repo (own manifest.json at its base URL).
    Seed {
        name: "vellum-icons",
        url: "https://nisugi.github.io/vellum-assets/icons",
        only: None,
    },
    Seed {
        name: "vellum-dolls",
        url: "https://nisugi.github.io/vellum-assets/dolls",
        only: None,
    },
    Seed {
        name: "vellum-skins",
        url: "https://nisugi.github.io/vellum-assets/skins",
        only: None,
    },
    Seed {
        name: "vellum-layouts",
        url: "https://nisugi.github.io/vellum-assets/layouts",
        only: None,
    },
    Seed {
        name: "vellum-frames",
        url: "https://nisugi.github.io/vellum-assets/frames",
        only: None,
    },
    Seed {
        name: "vellum-backgrounds",
        url: "https://nisugi.github.io/vellum-assets/backgrounds",
        only: None,
    },
    Seed {
        name: "vellum-compass",
        url: "https://nisugi.github.io/vellum-assets/compass",
        only: None,
    },
    Seed {
        name: "vellum-statusicons",
        url: "https://nisugi.github.io/vellum-assets/statusicons",
        only: None,
    },
    Seed {
        name: "vellum-hands",
        url: "https://nisugi.github.io/vellum-assets/hands",
        only: None,
    },
    // Creature-card base sprites for the creaturefield widget, resolved by
    // noun/family through [creature_card].resolve (GS4 feature).
    Seed {
        name: "vellum-creatures",
        url: "https://nisugi.github.io/vellum-assets/creatures",
        only: Some(GameType::GS4),
    },
];

/// Root discovery index of the vellum-assets monorepo: a repos.json listing
/// every category sub-repo, so a category added to the monorepo shows up in
/// `.jinx` without a client release. Merged (add-only) on top of the
/// compiled-in seeds each time the worker loads the repo list.
const DISCOVERY_INDEX: &str = "https://nisugi.github.io/vellum-assets/repos.json";

/// The repos.json shape: `{"repos":[{"name":"vellum-frames","url":"..."}]}`.
#[derive(Debug, Default, Deserialize)]
struct DiscoveryIndex {
    #[serde(default)]
    repos: Vec<RepoSource>,
}

/// Repo names to remove on load if present — deprecated or superseded seeds a
/// past version wrote to a user's `repos.toml`. Mirrors Jinx's `Setup.apply`
/// `repos_to_prune` (`jinx.lic:1244`): the list self-heals every existing
/// install without the user doing anything. Empty now that the vellum-assets
/// monorepo is live and its repos are seeded above — add a name here if a
/// seed's URL ever moves or is retired.
const PRUNE: &[&str] = &[];

impl RepoList {
    /// Load `repos.toml`, seeding and persisting the standard sources on first
    /// run (or topping up any missing seeds for the current game). `game` gates
    /// the game-specific seeds; `None` defaults to GS4 like the rest of the app.
    pub fn load_or_seed(game: Option<GameType>) -> anyhow::Result<RepoList> {
        let path = Config::base_dir()?.join("repos.toml");
        let mut list = if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            toml::from_str(&contents)?
        } else {
            RepoList::default()
        };
        let pruned = list.prune_deprecated();
        let seeded = list.apply_seeds(game.unwrap_or(GameType::GS4));
        if pruned || seeded {
            list.save()?;
        }
        Ok(list)
    }

    /// Remove any repo whose name is in [`PRUNE`]. Returns whether the list
    /// changed. Self-heals installs that a past version seeded with repos that
    /// don't exist yet.
    fn prune_deprecated(&mut self) -> bool {
        let before = self.repos.len();
        self.repos.retain(|r| !PRUNE.contains(&r.name.as_str()));
        self.repos.len() != before
    }

    /// Add any missing standard seeds for `game`. Returns whether the list
    /// changed. Never removes user-added repos, and never overwrites a repo the
    /// user has pointed elsewhere — only fills in absent standard names.
    fn apply_seeds(&mut self, game: GameType) -> bool {
        let mut changed = false;
        for seed in SEEDS {
            if seed.only.is_some_and(|g| g != game) {
                continue;
            }
            if !self.repos.iter().any(|r| r.name == seed.name) {
                self.repos.push(RepoSource {
                    name: seed.name.to_string(),
                    url: seed.url.to_string(),
                });
                changed = true;
            }
        }
        changed
    }

    /// Fetch the discovery index and merge any new repos, persisting on
    /// change. Best-effort on top of the compiled-in seeds: offline or a
    /// broken index just logs and moves on, never failing the command.
    pub fn discover(&mut self, agent: &ureq::Agent) {
        let index = match super::installer::fetch_bytes(agent, DISCOVERY_INDEX) {
            Ok(bytes) => match serde_json::from_slice::<DiscoveryIndex>(&bytes) {
                Ok(index) => index,
                Err(e) => {
                    tracing::warn!("jinx discovery index did not parse: {e}");
                    return;
                }
            },
            Err(e) => {
                tracing::debug!("jinx discovery index unavailable: {e}");
                return;
            }
        };
        if self.merge_discovered(index) {
            if let Err(e) = self.save() {
                tracing::warn!("could not persist discovered repos: {e}");
            }
        }
    }

    /// Add-only merge of index entries: existing names are never touched
    /// (user URL overrides win), non-HTTPS or empty-named entries are
    /// skipped. Returns whether the list changed.
    fn merge_discovered(&mut self, index: DiscoveryIndex) -> bool {
        let mut changed = false;
        for repo in index.repos {
            if repo.name.trim().is_empty() || !repo.url.starts_with("https://") {
                continue;
            }
            if self.find(&repo.name).is_none() {
                self.repos.push(RepoSource {
                    name: repo.name,
                    url: repo.url.trim_end_matches('/').to_string(),
                });
                changed = true;
            }
        }
        changed
    }

    /// Persist to `repos.toml` atomically (with a `.bak` backup), matching how
    /// every other user-authored config is written.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Config::base_dir()?.join("repos.toml");
        let toml = toml::to_string_pretty(self)?;
        crate::config::write_atomic(&path, toml)?;
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<&RepoSource> {
        self.repos.iter().find(|r| r.name == name)
    }

    /// Add a repository. Rejects a duplicate name and any non-HTTPS URL — a
    /// repo URL is a code-and-data source, so we don't fetch it over plain
    /// HTTP (Jinx applies the same rule).
    pub fn add(&mut self, name: &str, url: &str) -> Result<(), String> {
        if !url.starts_with("https://") {
            return Err(format!("repo URL must be https:// (got: {url})"));
        }
        if self.find(name).is_some() {
            return Err(format!("repo '{name}' already exists"));
        }
        self.repos.push(RepoSource {
            name: name.to_string(),
            url: url.trim_end_matches('/').to_string(),
        });
        Ok(())
    }

    /// Point an existing repo at a new URL.
    pub fn change(&mut self, name: &str, url: &str) -> Result<(), String> {
        if !url.starts_with("https://") {
            return Err(format!("repo URL must be https:// (got: {url})"));
        }
        let repo = self
            .repos
            .iter_mut()
            .find(|r| r.name == name)
            .ok_or_else(|| format!("repo '{name}' is not known"))?;
        repo.url = url.trim_end_matches('/').to_string();
        Ok(())
    }

    /// Remove a repo by name.
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        let before = self.repos.len();
        self.repos.retain(|r| r.name != name);
        if self.repos.len() == before {
            return Err(format!("repo '{name}' is not known"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_are_game_gated() {
        let mut gs = RepoList::default();
        assert!(gs.apply_seeds(GameType::GS4));
        assert!(gs.find("mapdb-backup-gs").is_some());
        assert!(gs.find("mapdb-backup-dr").is_none());
        assert!(gs.find("elanthia-online").is_some());
        // vellum-* repos are seeded (the vellum-assets monorepo is live).
        assert!(gs.find("vellum-skins").is_some());
        assert!(gs.find("vellum-icons").is_some());
        assert!(gs.find("vellum-dolls").is_some());
        assert!(gs.find("vellum-layouts").is_some());
        assert!(gs.find("vellum-frames").is_some());
        assert!(gs.find("vellum-backgrounds").is_some());
        assert!(gs.find("vellum-compass").is_some());
        assert!(gs.find("vellum-statusicons").is_some());

        let mut dr = RepoList::default();
        dr.apply_seeds(GameType::DR);
        assert!(dr.find("mapdb-backup-dr").is_some());
        assert!(dr.find("mapdb-backup-gs").is_none());
    }

    #[test]
    fn seeds_are_idempotent_and_preserve_user_repos() {
        let mut list = RepoList::default();
        list.add("myfriend", "https://example.com/skins").unwrap();
        assert!(list.apply_seeds(GameType::GS4)); // first fill: changed
        let count = list.repos.len();
        assert!(!list.apply_seeds(GameType::GS4)); // second: no change
        assert_eq!(list.repos.len(), count);
        assert!(list.find("myfriend").is_some()); // user repo untouched
    }

    #[test]
    fn prune_removes_exactly_the_deprecated_names_and_preserves_others() {
        // Drive the test off the real PRUNE list so it stays correct whether
        // PRUNE is empty (all seeds live) or lists retired names.
        let mut list = RepoList::default();
        list.add("myfriend", "https://example.com").unwrap();
        for name in PRUNE {
            list.repos.push(RepoSource {
                name: (*name).into(),
                url: "https://x".into(),
            });
        }

        let changed = list.prune_deprecated();
        assert_eq!(
            changed,
            !PRUNE.is_empty(),
            "changed iff there was something to prune"
        );
        for name in PRUNE {
            assert!(list.find(name).is_none(), "{name} should be pruned");
        }
        // User-added repos always survive.
        assert!(list.find("myfriend").is_some());
        // Idempotent: a second prune never changes anything.
        assert!(!list.prune_deprecated());
    }

    #[test]
    fn merge_discovered_is_add_only_and_filters_bad_entries() {
        let mut list = RepoList::default();
        list.add("vellum-frames", "https://mine.example/frames")
            .unwrap();

        let index = DiscoveryIndex {
            repos: vec![
                // Existing name: the user's URL must win.
                RepoSource {
                    name: "vellum-frames".into(),
                    url: "https://official/frames".into(),
                },
                // New name: merged in, trailing slash normalized.
                RepoSource {
                    name: "vellum-banners".into(),
                    url: "https://official/banners/".into(),
                },
                // Bad entries: skipped.
                RepoSource {
                    name: "".into(),
                    url: "https://official/x".into(),
                },
                RepoSource {
                    name: "vellum-evil".into(),
                    url: "http://insecure/evil".into(),
                },
            ],
        };
        assert!(list.merge_discovered(index));
        assert_eq!(
            list.find("vellum-frames").unwrap().url,
            "https://mine.example/frames"
        );
        assert_eq!(
            list.find("vellum-banners").unwrap().url,
            "https://official/banners"
        );
        assert!(list.find("vellum-evil").is_none());
        assert_eq!(list.repos.len(), 2);

        // Idempotent: merging the same index again changes nothing.
        let again = DiscoveryIndex {
            repos: vec![RepoSource {
                name: "vellum-banners".into(),
                url: "https://official/banners".into(),
            }],
        };
        assert!(!list.merge_discovered(again));
    }

    #[test]
    fn add_rejects_http_and_duplicates() {
        let mut list = RepoList::default();
        assert!(list.add("x", "http://insecure.example").is_err());
        list.add("x", "https://ok.example").unwrap();
        assert!(list.add("x", "https://ok.example/2").is_err()); // dup name
                                                                 // Trailing slash normalized off.
        list.add("y", "https://ok.example/y/").unwrap();
        assert_eq!(list.find("y").unwrap().url, "https://ok.example/y");
    }

    #[test]
    fn change_and_remove() {
        let mut list = RepoList::default();
        list.add("x", "https://a.example").unwrap();
        list.change("x", "https://b.example").unwrap();
        assert_eq!(list.find("x").unwrap().url, "https://b.example");
        assert!(list.change("missing", "https://c.example").is_err());
        list.remove("x").unwrap();
        assert!(list.find("x").is_none());
        assert!(list.remove("x").is_err());
    }

    #[test]
    fn roundtrips_through_toml() {
        let mut list = RepoList::default();
        list.apply_seeds(GameType::GS4);
        let toml = toml::to_string_pretty(&list).unwrap();
        let back: RepoList = toml::from_str(&toml).unwrap();
        assert_eq!(list.repos, back.repos);
    }
}
