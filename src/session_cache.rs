//! Session cache for data that is only sent at login (quickbars, etc.).
//!
//! This allows VellumFE to attach to a running Lich session without losing
//! quickbar/spells content that is not resent after login.

use crate::config::Config;
use crate::data::QuickbarData;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const CACHE_FILENAME: &str = "session_cache.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionCache {
    #[serde(default)]
    pub quickbars: HashMap<String, QuickbarData>,
    #[serde(default)]
    pub quickbar_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_quickbar_id: Option<String>,
}

fn cache_path(character: Option<&str>) -> Result<PathBuf> {
    let profile_dir = Config::profile_dir(character)?;
    Ok(profile_dir.join(CACHE_FILENAME))
}

pub fn load(character: Option<&str>) -> Option<SessionCache> {
    let path = cache_path(character).ok()?;
    let contents = fs::read_to_string(&path).ok()?;
    match toml::from_str(&contents) {
        Ok(cache) => Some(cache),
        Err(err) => {
            tracing::warn!(
                "Failed to parse session cache at {:?}: {}",
                path,
                err
            );
            None
        }
    }
}

pub fn save(character: Option<&str>, cache: &SessionCache) -> Result<()> {
    let path = cache_path(character)?;
    let contents = toml::to_string_pretty(cache).context("Failed to serialize session cache")?;
    fs::write(&path, contents).context("Failed to write session cache")?;
    Ok(())
}
