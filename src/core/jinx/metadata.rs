//! What's installed, for update detection.
//!
//! `~/.vellum-fe/jinx-installed.toml` records, per asset the manager installed,
//! the repo it came from and the digest that was delivered. `auto-update` diffs
//! each record's `digest` against the current manifest `md5`; a mismatch means
//! an update is available. The digest is over the delivered file (the plain
//! file, or the whole bundle zip), so any change to any file in a composed
//! skin flips it — one digest covers the whole asset.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::Config;

/// One installed asset's tracking record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledAsset {
    pub repo: String,
    /// base64(SHA1) of the delivered file, as verified at install time.
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub kind: String,
    /// Unix timestamp of the last install/update. Absent on records written
    /// before timestamping existed (the panel shows "unknown" age, the login
    /// nudge treats them as stale). Set by the worker on every install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<i64>,
}

/// The `jinx-installed.toml` document: basename -> record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledDb {
    #[serde(default, rename = "asset")]
    pub assets: BTreeMap<String, InstalledAsset>,
}

impl InstalledDb {
    pub fn load() -> anyhow::Result<InstalledDb> {
        let path = Config::base_dir()?.join("jinx-installed.toml");
        if !path.exists() {
            return Ok(InstalledDb::default());
        }
        let contents = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Config::base_dir()?.join("jinx-installed.toml");
        let toml = toml::to_string_pretty(self)?;
        crate::config::write_atomic(&path, toml)?;
        Ok(())
    }

    pub fn record(&mut self, name: &str, asset: InstalledAsset) {
        self.assets.insert(name.to_string(), asset);
    }

    pub fn get(&self, name: &str) -> Option<&InstalledAsset> {
        self.assets.get(name)
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.assets.remove(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_roundtrip() {
        let mut db = InstalledDb::default();
        db.record(
            "parchment.vellumpack",
            InstalledAsset {
                repo: "vellum-skins".into(),
                digest: "abc=".into(),
                version: Some("1.2.0".into()),
                kind: "skin".into(),
                last_updated: Some(1_700_000_000),
            },
        );
        db.record(
            "gameobj-data.xml",
            InstalledAsset {
                repo: "elanthia-online".into(),
                digest: "def=".into(),
                version: None,
                kind: "data".into(),
                last_updated: None,
            },
        );
        let toml = toml::to_string_pretty(&db).unwrap();
        let back: InstalledDb = toml::from_str(&toml).unwrap();
        assert_eq!(back.assets.len(), 2);
        assert_eq!(
            back.get("parchment.vellumpack").unwrap().version.as_deref(),
            Some("1.2.0")
        );
        assert!(back.get("gameobj-data.xml").unwrap().version.is_none());

        let mut db = back;
        assert!(db.remove("gameobj-data.xml"));
        assert!(!db.remove("gameobj-data.xml"));
    }

    #[test]
    fn update_available_is_digest_mismatch() {
        let mut db = InstalledDb::default();
        db.record(
            "x.xml",
            InstalledAsset {
                repo: "r".into(),
                digest: "old=".into(),
                version: None,
                kind: "data".into(),
                last_updated: None,
            },
        );
        // Same digest -> up to date; different -> update available.
        assert_eq!(db.get("x.xml").unwrap().digest, "old=");
        let manifest_md5 = "new=";
        assert_ne!(db.get("x.xml").unwrap().digest, manifest_md5);
    }
}
