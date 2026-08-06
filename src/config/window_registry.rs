//! Discovery-memory registry — every game binding this character has ever
//! seen, persisted as `window_registry.toml` beside the layout.
//!
//! Window-system redesign Phase 1b (spec:
//! `.beads/artifacts/window-system-redesign/spec.md`). A separate file,
//! deliberately: builds from before this feature never read it, so it can
//! never break an old install. DARK in Phase 1 — written from the
//! discovery pipeline, consumed by nothing. Phase 3's Windows-list union
//! reads it so "Bounty" is addable in a fresh layout before the game
//! re-declares it this session, replacing the template catalog's phantom
//! rows.
//!
//! Seeded on first run from the well-known GS4/DR feeds (transcribed from
//! the soon-to-die static maps + the wire-verified id inventory); the
//! seed constant dies with the catalog in Phase 6, the registry persists.

use super::{write_atomic, Config};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowRegistry {
    #[serde(default)]
    pub bindings: Vec<RegistryBinding>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegistryBinding {
    /// "stream" | "dialog" — a plain string so kinds added later (popup,
    /// container) round-trip through builds that don't know them.
    pub kind: String,
    /// The game id (dialog/stream id) — the identity key.
    pub id: String,
    /// Last title the game declared for it (display only; may improve
    /// across sessions).
    #[serde(default)]
    pub title: String,
}

/// Well-known feeds every character can add before the game first speaks:
/// (kind, id, title). Transcribed from the template catalog's stream
/// subscriptions and the dialog id inventory verified against 11.4 GB of
/// wire logs.
const WELL_KNOWN: &[(&str, &str, &str)] = &[
    ("stream", "thoughts", "Thoughts"),
    ("stream", "speech", "Speech"),
    ("stream", "familiar", "Familiar"),
    ("stream", "logons", "Logons"),
    ("stream", "death", "Deaths"),
    ("stream", "loot", "Loot"),
    ("stream", "bounty", "Bounty"),
    ("stream", "society", "Society"),
    ("stream", "ambients", "Ambients"),
    ("stream", "announcements", "Announcements"),
    ("stream", "voln", "Voln"),
    ("stream", "inv", "Inventory"),
    ("stream", "reserve", "Reserve"),
    ("stream", "room", "Room"),
    ("stream", "Spells", "Spells"),
    ("dialog", "minivitals", "Vitals"),
    ("dialog", "IconBAR", "Status"),
    ("dialog", "expr", "Experience"),
    ("dialog", "encum", "Encumbrance"),
    ("dialog", "stance", "Stance"),
    ("dialog", "injuries", "Injuries"),
    ("dialog", "Buffs", "Buffs"),
    ("dialog", "Debuffs", "Debuffs"),
    ("dialog", "Cooldowns", "Cooldowns"),
    ("dialog", "Active Spells", "Active Spells"),
    ("dialog", "combat", "Combat"),
    ("dialog", "bank", "Bank"),
    ("dialog", "befriend", "Befriend"),
];

impl WindowRegistry {
    /// Record a seen binding. Identity is (kind, id); a fresh sighting of
    /// a known binding only refreshes its title. Returns whether anything
    /// changed (the caller persists on true).
    pub fn record(&mut self, kind: &str, id: &str, title: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        if let Some(existing) = self
            .bindings
            .iter_mut()
            .find(|b| b.kind == kind && b.id == id)
        {
            if !title.is_empty() && existing.title != title {
                existing.title = title.to_string();
                return true;
            }
            return false;
        }
        self.bindings.push(RegistryBinding {
            kind: kind.to_string(),
            id: id.to_string(),
            title: title.to_string(),
        });
        true
    }

    /// First-run seed: add every well-known feed not already present.
    /// Idempotent; never overwrites titles the game has since declared.
    pub fn seed_well_known(&mut self) -> bool {
        let mut changed = false;
        for (kind, id, title) in WELL_KNOWN {
            let known = self.bindings.iter().any(|b| b.kind == *kind && b.id == *id);
            if !known {
                self.bindings.push(RegistryBinding {
                    kind: kind.to_string(),
                    id: id.to_string(),
                    title: title.to_string(),
                });
                changed = true;
            }
        }
        changed
    }
}

impl Config {
    /// `~/.vellum-fe/{character}/window_registry.toml`
    pub fn window_registry_path(character: Option<&str>) -> Result<PathBuf> {
        Ok(Self::profile_dir(character)?.join("window_registry.toml"))
    }

    /// Load the registry; a missing file is an empty registry, and a
    /// corrupt one is treated the same (with a warning) — discovery
    /// memory must never block a session.
    pub fn load_window_registry(character: Option<&str>) -> WindowRegistry {
        let Ok(path) = Self::window_registry_path(character) else {
            return WindowRegistry::default();
        };
        if !path.exists() {
            return WindowRegistry::default();
        }
        match std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|contents| toml::from_str(&contents).map_err(anyhow::Error::from))
        {
            Ok(registry) => registry,
            Err(e) => {
                tracing::warn!("window_registry.toml unreadable ({e}); starting empty");
                WindowRegistry::default()
            }
        }
    }

    pub fn save_window_registry(
        character: Option<&str>,
        registry: &WindowRegistry,
    ) -> Result<()> {
        let path = Self::window_registry_path(character)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents =
            toml::to_string_pretty(registry).context("serialize window registry")?;
        write_atomic(&path, contents).context("write window_registry.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_dedups_on_kind_and_id_and_refreshes_title() {
        let mut registry = WindowRegistry::default();
        assert!(registry.record("stream", "bounty", "Bounty"));
        assert!(!registry.record("stream", "bounty", "Bounty"), "no change");
        // Same id under a different kind is a different binding.
        assert!(registry.record("dialog", "bounty", "Bounty Dialog"));
        // A better title refreshes in place, no duplicate row.
        assert!(registry.record("stream", "bounty", "Adventurer's Guild"));
        assert_eq!(
            registry
                .bindings
                .iter()
                .filter(|b| b.kind == "stream" && b.id == "bounty")
                .count(),
            1
        );
        // Empty sightings never register or clobber.
        assert!(!registry.record("stream", "", "x"));
        assert!(!registry.record("stream", "bounty", ""));
        assert_eq!(
            registry
                .bindings
                .iter()
                .find(|b| b.kind == "stream" && b.id == "bounty")
                .unwrap()
                .title,
            "Adventurer's Guild"
        );
    }

    #[test]
    fn seed_is_idempotent_and_respects_seen_titles() {
        let mut registry = WindowRegistry::default();
        assert!(registry.record("stream", "bounty", "Adventurer's Guild"));
        assert!(registry.seed_well_known());
        let count = registry.bindings.len();
        assert!(!registry.seed_well_known(), "second seed is a no-op");
        assert_eq!(registry.bindings.len(), count);
        assert_eq!(
            registry
                .bindings
                .iter()
                .find(|b| b.kind == "stream" && b.id == "bounty")
                .unwrap()
                .title,
            "Adventurer's Guild",
            "seed never overwrites a game-declared title"
        );
    }

    #[test]
    fn registry_toml_round_trips_and_tolerates_unknown_kinds() {
        let mut registry = WindowRegistry::default();
        registry.seed_well_known();
        let toml = toml::to_string_pretty(&registry).unwrap();
        let back: WindowRegistry = toml::from_str(&toml).unwrap();
        assert_eq!(back, registry);

        // A future build's kind (popup/container) parses fine here.
        let future: WindowRegistry = toml::from_str(
            "[[bindings]]\nkind = \"popup\"\nid = \"bank\"\ntitle = \"Bank\"\n",
        )
        .unwrap();
        assert_eq!(future.bindings[0].kind, "popup");
    }
}
