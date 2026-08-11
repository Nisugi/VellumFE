//! Layered config loading and sparse, comment-preserving saves.
//!
//! Loading layers per-LEAF: embedded defaults < global/config.toml <
//! profiles/{char}/config.toml. Only keys a file actually states override
//! the layer below — unlike the old `merge_with`, which replaced whole
//! sections, so any profile file at all cut the character off from every
//! global scalar.
//!
//! Saving is the inverse: a config file only ever contains the keys that
//! DIFFER from its inherited layer. Saves edit the existing document via
//! toml_edit (hand-written comments on surviving keys are preserved),
//! update diverged keys, insert new divergences, and remove keys that now
//! merely restate the inherited value. Full-dump profiles from the old
//! GUI save thin back down to real overrides on their next save.

use super::Config;
use anyhow::{Context, Result};
use toml_edit::DocumentMut;

/// Deep-merge `overlay` into `base`: tables recurse, everything else
/// (scalars, arrays) replaces wholesale.
fn deep_merge(base: &mut toml::Value, overlay: &toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_tbl), toml::Value::Table(overlay_tbl)) => {
            for (key, overlay_child) in overlay_tbl {
                match base_tbl.get_mut(key) {
                    Some(base_child) => deep_merge(base_child, overlay_child),
                    None => {
                        base_tbl.insert(key.clone(), overlay_child.clone());
                    }
                }
            }
        }
        (base_slot, overlay_value) => *base_slot = overlay_value.clone(),
    }
}

/// Build a Config by layering raw file contents (per-leaf) over the code
/// defaults. `None` layers are skipped. Pure — callers do the file IO.
pub(super) fn layer_config(global: Option<&str>, profile: Option<&str>) -> Result<Config> {
    let mut value =
        toml::Value::try_from(Config::default()).context("Config defaults must serialize")?;
    for (name, layer) in [("global", global), ("profile", profile)] {
        if let Some(text) = layer {
            let overlay: toml::Value = toml::from_str(text)
                .with_context(|| format!("Failed to parse {} config layer", name))?;
            deep_merge(&mut value, &overlay);
        }
    }
    scrub_cleared_options(&mut value, "");
    value.try_into().context("Layered config did not deserialize")
}

/// Resolves the cleared-Option sentinel at load. TOML has no null, so
/// `diff_value` records a cleared Option as `""`; only a couple of fields
/// decode that themselves via `empty_string_as_none`. Every other
/// `OptionalText` setting would deserialize the sentinel as `Some("")` —
/// visibly wrong downstream (`base.join("")`, a '' voice warning) instead of
/// reverting to the default. Deleting the key here makes serde default the
/// field to `None`, for every OptionalText setting at once. `Text` settings
/// are untouched: for them an empty string is a legitimate value.
fn scrub_cleared_options(value: &mut toml::Value, path: &str) {
    let toml::Value::Table(tbl) = value else {
        return;
    };
    let keys: Vec<String> = tbl.keys().cloned().collect();
    for key in keys {
        let dotted = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        let Some(child) = tbl.get_mut(&key) else {
            continue;
        };
        let is_cleared_option = child.as_str() == Some("")
            && matches!(
                super::registry::find(&dotted).map(|def| &def.kind),
                Some(super::registry::SettingKind::OptionalText)
            );
        if is_cleared_option {
            tbl.remove(&key);
        } else {
            scrub_cleared_options(child, &dotted);
        }
    }
}

/// The keys of `config` that differ from `base`, as a nested TOML table.
/// Identical subtrees vanish entirely. Returns None when nothing differs.
///
/// A registered key the base states and `config` omits is a cleared Option
/// (`.setskin none`). TOML has no null, so it is written as an empty
/// string — the encoding the settings registry already documents ("empty =
/// ...") and that readers like `uipack` already treat as unset. Staying
/// silent instead would re-inherit the base's value on the next load.
fn diff_value(config: &toml::Value, base: &toml::Value, path: &str) -> Option<toml::Value> {
    match (config, base) {
        (toml::Value::Table(cfg_tbl), toml::Value::Table(base_tbl)) => {
            let mut out = toml::value::Table::new();
            let dotted = |key: &str| {
                if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", path, key)
                }
            };
            for (key, cfg_child) in cfg_tbl {
                match base_tbl.get(key) {
                    Some(base_child) => {
                        if let Some(changed) = diff_value(cfg_child, base_child, &dotted(key)) {
                            out.insert(key.clone(), changed);
                        }
                    }
                    None => {
                        out.insert(key.clone(), cfg_child.clone());
                    }
                }
            }
            // Cleared Options: stated by the base, omitted by config.
            for key in base_tbl.keys() {
                if cfg_tbl.contains_key(key) {
                    continue;
                }
                if super::registry::find(&dotted(key)).is_some() {
                    out.insert(key.clone(), toml::Value::String(String::new()));
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(toml::Value::Table(out))
            }
        }
        _ if config == base => None,
        _ => Some(config.clone()),
    }
}

/// Remove document keys whose value merely restates the inherited layer
/// (they are no-op overrides), recursing into tables. Keys the Config
/// serialization doesn't know about are left untouched — hand-authored
/// extras survive.
///
/// `path` is the dotted prefix of the table being pruned, so a key absent
/// from `config` can be checked against the settings registry. A `None`
/// Option is omitted from `config` entirely, which used to be
/// indistinguishable from a hand-authored key: clearing an Option
/// (`.setskin none` -> active_skin = None) left the stale line in the file
/// and the next load resurrected the old value.
fn prune_inherited(
    doc: &mut toml_edit::Table,
    config: &toml::Value,
    base: &toml::Value,
    path: &str,
) {
    let empty = toml::Value::Table(toml::value::Table::new());
    let keys: Vec<String> = doc.iter().map(|(key, _)| key.to_string()).collect();
    for key in keys {
        let dotted = if path.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", path, key)
        };
        let Some(cfg_child) = config.get(key.as_str()) else {
            // A registered key that Config omitted is a cleared Option.
            // The line must go, or it outlives the value it recorded.
            if super::registry::find(&dotted).is_some() {
                doc.remove(&key);
            }
            continue; // otherwise unknown: not ours to manage
        };
        let base_child = base.get(key.as_str());
        if base_child == Some(cfg_child) {
            doc.remove(&key);
            continue;
        }
        if cfg_child.is_table() {
            if let Some(child_tbl) = doc.get_mut(&key).and_then(|item| item.as_table_mut()) {
                prune_inherited(child_tbl, cfg_child, base_child.unwrap_or(&empty), &dotted);
                if child_tbl.is_empty() {
                    doc.remove(&key);
                }
            }
        }
    }
}

/// Overwrite/insert the diffed keys into the document, recursing into
/// tables so sibling keys (and their comments) survive.
fn merge_items(doc: &mut toml_edit::Table, src: &toml_edit::Table) {
    for (key, item) in src.iter() {
        let recursed = match (
            doc.get_mut(key).and_then(|existing| existing.as_table_mut()),
            item.as_table(),
        ) {
            (Some(doc_tbl), Some(src_tbl)) => {
                merge_items(doc_tbl, src_tbl);
                true
            }
            _ => false,
        };
        if !recursed {
            doc.insert(key, item.clone());
        }
    }
}

/// Produce the new contents of a config file so that it states exactly
/// the keys where `config` diverges from `base`, editing `existing` in
/// place (comments on surviving keys are preserved). Pure — callers do
/// the file IO.
pub(super) fn sparse_config_toml(existing: &str, config: &Config, base: &Config) -> Result<String> {
    let cfg_value = toml::Value::try_from(config).context("Config must serialize")?;
    let base_value = toml::Value::try_from(base).context("Base config must serialize")?;

    let mut doc: DocumentMut = existing
        .parse()
        .context("Existing config file did not parse as TOML")?;

    prune_inherited(doc.as_table_mut(), &cfg_value, &base_value, "");

    if let Some(diff) = diff_value(&cfg_value, &base_value, "") {
        let rendered = toml::to_string(&diff).context("Diff must serialize")?;
        let diff_doc: DocumentMut = rendered
            .parse()
            .context("Serialized diff must re-parse")?;
        merge_items(doc.as_table_mut(), diff_doc.as_table());
    }

    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layering_is_per_leaf_not_per_section() {
        // Global sets a ui scalar; the profile file mentions only
        // connection. The old merge_with would have reset ui to defaults.
        let global = "[ui]\nbuffer_size = 5000\n";
        let profile = "[connection]\nhost = \"10.0.0.5\"\n";
        let layered = layer_config(Some(global), Some(profile)).unwrap();
        assert_eq!(layered.ui.buffer_size, 5000);
        assert_eq!(layered.connection.host, "10.0.0.5");
        // Untouched leaves fall through to defaults.
        assert_eq!(layered.connection.port, Config::default().connection.port);
    }

    #[test]
    fn profile_layer_wins_over_global() {
        let global = "[ui]\nbuffer_size = 5000\n";
        let profile = "[ui]\nbuffer_size = 200\n";
        let layered = layer_config(Some(global), Some(profile)).unwrap();
        assert_eq!(layered.ui.buffer_size, 200);
    }

    #[test]
    fn sparse_save_writes_only_divergences() {
        let base = Config::default();
        let mut config = Config::default();
        config.ui.buffer_size = 4321;
        let out = sparse_config_toml("", &config, &base).unwrap();
        assert!(out.contains("buffer_size = 4321"), "{}", out);
        // Nothing else leaks in: no sections beyond [ui].
        assert!(!out.contains("[sound]"), "{}", out);
        assert!(!out.contains("[connection]"), "{}", out);
    }

    #[test]
    fn sparse_save_thins_full_dumps_and_keeps_foreign_keys() {
        // A full old-style dump of the defaults, plus a hand-written
        // comment + unknown key the config layer knows nothing about.
        let base = Config::default();
        let full_dump = toml::to_string_pretty(&Config::default()).unwrap();
        let existing = format!("# my notes\ncustom_key = \"mine\"\n{}", full_dump);

        let mut config = Config::default();
        config.sound.volume = 0.25;
        let out = sparse_config_toml(&existing, &config, &base).unwrap();

        // Redundant restatements of inherited values are gone...
        assert!(!out.contains("buffer_size"), "{}", out);
        assert!(!out.contains("[connection]"), "{}", out);
        // ...the real override and the user's own material survive.
        assert!(out.contains("volume = 0.25"), "{}", out);
        assert!(out.contains("# my notes"), "{}", out);
        assert!(out.contains("custom_key = \"mine\""), "{}", out);
    }

    #[test]
    fn sparse_save_removes_override_reset_to_inherited() {
        let mut base = Config::default();
        base.ui.buffer_size = 5000; // inherited from global layer
        let existing = "[ui]\nbuffer_size = 200\n";
        // The user reset the profile value back to the inherited one.
        let mut config = Config::default();
        config.ui.buffer_size = 5000;
        let out = sparse_config_toml(existing, &config, &base).unwrap();
        assert!(!out.contains("buffer_size"), "{}", out);
    }

    /// TOML has no null, so a cleared Option is saved as "". Loading must turn
    /// that sentinel back into None for EVERY OptionalText setting — before the
    /// scrub, only active_skin/doll_image decoded it, and e.g. a cleared
    /// logging.dir came back as Some("") (logs landed in `base.join("")`).
    #[test]
    fn cleared_option_sentinel_loads_as_none() {
        let base = Config::default();
        let mut with_dir = Config::default();
        with_dir.logging.dir = Some("mylogs".to_string());
        // The global layer sets the option; the profile clears it.
        let global = sparse_config_toml("", &with_dir, &base).unwrap();
        assert!(global.contains("mylogs"), "{}", global);

        let profile = "[logging]\ndir = \"\"\n";
        let layered = layer_config(Some(&global), Some(profile)).unwrap();
        assert_eq!(
            layered.logging.dir, None,
            "the \"\" sentinel must clear the option, not become Some(\"\")"
        );

        // tts.voice rides the same rule.
        let layered = layer_config(None, Some("[tts]\nvoice = \"\"\n")).unwrap();
        assert_eq!(layered.tts.voice, None);
    }

    /// Clearing an Option in a profile and saving must round-trip: the save
    /// writes the sentinel, the load resolves it to None.
    #[test]
    fn cleared_option_round_trips_through_save_and_load() {
        let mut base = Config::default();
        base.logging.dir = Some("global-dir".to_string()); // inherited layer
        let mut config = Config::default();
        config.logging.dir = None; // user cleared it in the profile
        let existing = "[logging]\ndir = \"global-dir\"\n";
        let saved = sparse_config_toml(existing, &config, &base).unwrap();
        assert!(saved.contains("dir = \"\""), "{}", saved);

        // Loading global (Some) + profile (sentinel) yields None, not Some("").
        let global = "[logging]\ndir = \"global-dir\"\n";
        let layered = layer_config(Some(global), Some(&saved)).unwrap();
        assert_eq!(layered.logging.dir, None);
    }

    #[test]
    fn sparse_then_layer_round_trips() {
        // Whatever sparse-save writes, layering must read back to the
        // same effective config.
        let base = Config::default();
        let mut config = Config::default();
        config.ui.buffer_size = 777;
        config.tts.enabled = true;
        config.web.bind = "0.0.0.0".to_string();
        let saved = sparse_config_toml("", &config, &base).unwrap();
        let reloaded = layer_config(None, Some(&saved)).unwrap();
        assert_eq!(reloaded.ui.buffer_size, 777);
        assert!(reloaded.tts.enabled);
        assert_eq!(reloaded.web.bind, "0.0.0.0");
    }
}
