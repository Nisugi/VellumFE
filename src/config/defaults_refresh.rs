//! Additive refresh of shipped collection defaults.
//!
//! Collection files (highlights, keybinds `[user]`, hotbar bars) extract to
//! disk once and then belong to the user — so new defaults shipped in a
//! release never used to reach existing installs. This module fixes that
//! without resurrecting deletions: a sidecar
//! (`~/.vellum-fe/global/.defaults-seen.toml`) records every default entry
//! key this install has already been offered. On startup, embedded entries
//! whose key was never seen AND is absent from the user's file are appended
//! (comment-preserving, via toml_edit); keys the user deleted stay deleted
//! because they were seen when first offered.
//!
//! Existing installs (no sidecar yet) seed the seen-set from the union of
//! embedded and on-disk keys: refresh applies forward from that point, and
//! nothing is retroactively added or resurrected.
//!
//! Data files nobody hand-edits (cmdlist1.xml, spell_abbrev.toml) refresh
//! by content hash instead: while the on-disk file still matches the hash
//! we recorded at extraction, a changed embedded copy overwrites it; the
//! moment the user modifies the file, the hash mismatch parks it forever.

use super::write_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use toml_edit::DocumentMut;

/// Stable, dependency-free FNV-1a 64 content hash (std's DefaultHasher is
/// not stable across releases, which would silently park managed files).
fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SeenDefaults {
    /// Default highlight names already offered (None = never seeded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    highlights: Option<BTreeSet<String>>,
    /// Default `[user]` keybind combos already offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_keybinds: Option<BTreeSet<String>>,
    /// Default hotbar names already offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hotbars: Option<BTreeSet<String>>,
    /// Content hashes for managed (never-hand-edited) data files.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    managed_hashes: BTreeMap<String, String>,
}

/// Outcome of refreshing one collection: the rewritten file (None = no
/// change needed) and the new seen-set to record.
struct RefreshOutcome {
    updated_file: Option<String>,
    seen: BTreeSet<String>,
}

/// Shared core: given the embedded entry keys, the user file's entry keys,
/// and the previously-seen set, decide which keys to append. `seen: None`
/// seeds forward-only (existing install, first sidecar).
fn keys_to_add(
    embedded: &BTreeSet<String>,
    on_disk: &BTreeSet<String>,
    seen: Option<&BTreeSet<String>>,
) -> Vec<String> {
    match seen {
        None => Vec::new(), // seeding pass: offer nothing retroactively
        Some(seen) => embedded
            .iter()
            .filter(|key| !seen.contains(*key) && !on_disk.contains(*key))
            .cloned()
            .collect(),
    }
}

fn merged_seen(
    embedded: &BTreeSet<String>,
    on_disk: &BTreeSet<String>,
    seen: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut merged: BTreeSet<String> = embedded.clone();
    match seen {
        // Seeding: on-disk keys count as offered too, so a later default
        // that collides with a user-authored name is never clobbered.
        None => merged.extend(on_disk.iter().cloned()),
        Some(seen) => merged.extend(seen.iter().cloned()),
    }
    merged
}

/// Highlights: entries are top-level named tables.
fn refresh_named_tables(
    user: &str,
    embedded: &str,
    seen: Option<&BTreeSet<String>>,
) -> Result<RefreshOutcome> {
    let mut user_doc: DocumentMut = user.parse().context("user file did not parse")?;
    let embedded_doc: DocumentMut = embedded.parse().context("embedded defaults did not parse")?;

    let embedded_keys: BTreeSet<String> = embedded_doc
        .as_table()
        .iter()
        .filter(|(_, item)| item.is_table())
        .map(|(key, _)| key.to_string())
        .collect();
    let on_disk: BTreeSet<String> = user_doc
        .as_table()
        .iter()
        .map(|(key, _)| key.to_string())
        .collect();

    let to_add = keys_to_add(&embedded_keys, &on_disk, seen);
    let updated_file = if to_add.is_empty() {
        None
    } else {
        for key in &to_add {
            if let Some(item) = embedded_doc.get(key) {
                user_doc.as_table_mut().insert(key, item.clone());
            }
        }
        Some(user_doc.to_string())
    };
    Ok(RefreshOutcome {
        updated_file,
        seen: merged_seen(&embedded_keys, &on_disk, seen),
    })
}

/// Keybinds: entries live under the `[user]` and `[controller]` tables.
/// `[controller]` keys carry a `controller/` prefix in the seen-set so
/// they can't collide with `[user]` combos (sidecars from before the
/// controller table remain valid unchanged).
fn refresh_user_keybinds(
    user: &str,
    embedded: &str,
    seen: Option<&BTreeSet<String>>,
) -> Result<RefreshOutcome> {
    let mut user_doc: DocumentMut = user.parse().context("user keybinds did not parse")?;
    let embedded_doc: DocumentMut = embedded.parse().context("embedded keybinds did not parse")?;

    let table_keys = |doc: &DocumentMut, table: &str, prefix: &str| -> BTreeSet<String> {
        doc.get(table)
            .and_then(|item| item.as_table())
            .map(|tbl| {
                tbl.iter()
                    .map(|(key, _)| format!("{}{}", prefix, key))
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut embedded_keys = BTreeSet::new();
    let mut on_disk = BTreeSet::new();
    let mut changed = false;

    for (table, prefix) in [
        ("user", ""),
        ("controller", "controller/"),
        ("controller_shift", "controller_shift/"),
    ] {
        let embedded_tbl_keys = table_keys(&embedded_doc, table, prefix);
        let on_disk_tbl_keys = table_keys(&user_doc, table, prefix);
        let to_add = keys_to_add(&embedded_tbl_keys, &on_disk_tbl_keys, seen);

        if !to_add.is_empty() {
            if user_doc.get(table).and_then(|item| item.as_table()).is_none() {
                user_doc
                    .as_table_mut()
                    .insert(table, toml_edit::Item::Table(toml_edit::Table::new()));
            }
            let user_tbl = user_doc[table]
                .as_table_mut()
                .expect("just ensured the table exists");
            let embedded_tbl = embedded_doc
                .get(table)
                .and_then(|item| item.as_table())
                .expect("embedded keybinds have the table");
            for prefixed in &to_add {
                let key = prefixed.strip_prefix(prefix).unwrap_or(prefixed);
                if let Some(item) = embedded_tbl.get(key) {
                    user_tbl.insert(key, item.clone());
                    changed = true;
                }
            }
        }

        embedded_keys.extend(embedded_tbl_keys);
        on_disk.extend(on_disk_tbl_keys);
    }

    // [[controller_wheel]] slices refresh additively by label, with
    // wheel/-prefixed seen keys (deleted slices stay deleted).
    {
        let wheel_labels = |doc: &DocumentMut| -> BTreeSet<String> {
            doc.get("controller_wheel")
                .and_then(|item| item.as_array_of_tables())
                .map(|slices| {
                    slices
                        .iter()
                        .filter_map(|t| t.get("label").and_then(|l| l.as_str()))
                        .map(|l| format!("wheel/{}", l))
                        .collect()
                })
                .unwrap_or_default()
        };
        let embedded_wheel = wheel_labels(&embedded_doc);
        let on_disk_wheel = wheel_labels(&user_doc);
        let to_add = keys_to_add(&embedded_wheel, &on_disk_wheel, seen);
        if !to_add.is_empty() {
            if user_doc
                .get("controller_wheel")
                .and_then(|item| item.as_array_of_tables())
                .is_none()
            {
                user_doc.as_table_mut().insert(
                    "controller_wheel",
                    toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
                );
            }
            let embedded_slices = embedded_doc
                .get("controller_wheel")
                .and_then(|item| item.as_array_of_tables())
                .expect("embedded keybinds have [[controller_wheel]]");
            let user_slices = user_doc["controller_wheel"]
                .as_array_of_tables_mut()
                .expect("just ensured [[controller_wheel]] exists");
            for slice in embedded_slices.iter() {
                let label = slice.get("label").and_then(|l| l.as_str()).unwrap_or("");
                if to_add.iter().any(|add| add == &format!("wheel/{}", label)) {
                    user_slices.push(slice.clone());
                    changed = true;
                }
            }
        }
        embedded_keys.extend(embedded_wheel);
        on_disk.extend(on_disk_wheel);
    }

    let updated_file = changed.then(|| user_doc.to_string());
    Ok(RefreshOutcome {
        updated_file,
        seen: merged_seen(&embedded_keys, &on_disk, seen),
    })
}

/// Hotbars: entries are `[[bars]]` tables identified by their `name`.
fn refresh_bars(
    user: &str,
    embedded: &str,
    seen: Option<&BTreeSet<String>>,
) -> Result<RefreshOutcome> {
    let mut user_doc: DocumentMut = user.parse().context("user hotbars did not parse")?;
    let embedded_doc: DocumentMut = embedded.parse().context("embedded hotbars did not parse")?;

    let bar_names = |doc: &DocumentMut| -> BTreeSet<String> {
        doc.get("bars")
            .and_then(|item| item.as_array_of_tables())
            .map(|bars| {
                bars.iter()
                    .filter_map(|bar| bar.get("name").and_then(|name| name.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let embedded_keys = bar_names(&embedded_doc);
    let on_disk = bar_names(&user_doc);

    let to_add = keys_to_add(&embedded_keys, &on_disk, seen);
    let updated_file = if to_add.is_empty() {
        None
    } else {
        if user_doc
            .get("bars")
            .and_then(|item| item.as_array_of_tables())
            .is_none()
        {
            user_doc.as_table_mut().insert(
                "bars",
                toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
            );
        }
        let embedded_bars = embedded_doc
            .get("bars")
            .and_then(|item| item.as_array_of_tables())
            .expect("embedded hotbars have [[bars]]");
        let user_bars = user_doc["bars"]
            .as_array_of_tables_mut()
            .expect("just ensured [[bars]] exists");
        for bar in embedded_bars.iter() {
            let name = bar.get("name").and_then(|name| name.as_str()).unwrap_or("");
            if to_add.iter().any(|add| add == name) {
                user_bars.push(bar.clone());
            }
        }
        Some(user_doc.to_string())
    };
    Ok(RefreshOutcome {
        updated_file,
        seen: merged_seen(&embedded_keys, &on_disk, seen),
    })
}

/// Startup entry point: refresh the global collection files and managed
/// data files against the embedded defaults, then persist the sidecar.
/// Cheap when nothing changed (string parses, no writes).
pub(super) fn refresh_shipped_defaults() -> Result<()> {
    let global_dir = super::Config::global_dir()?;
    let sidecar_path = global_dir.join(".defaults-seen.toml");
    let mut sidecar: SeenDefaults = match std::fs::read_to_string(&sidecar_path) {
        Ok(text) => toml::from_str(&text).unwrap_or_default(),
        Err(_) => SeenDefaults::default(),
    };
    let mut sidecar_dirty = false;

    let collections: [(&str, &str, fn(&str, &str, Option<&BTreeSet<String>>) -> Result<RefreshOutcome>, fn(&mut SeenDefaults) -> &mut Option<BTreeSet<String>>); 3] = [
        ("highlights.toml", super::DEFAULT_HIGHLIGHTS, refresh_named_tables, |s| &mut s.highlights),
        ("keybinds.toml", super::DEFAULT_KEYBINDS, refresh_user_keybinds, |s| &mut s.user_keybinds),
        ("hotbars.toml", super::DEFAULT_HOTBARS, refresh_bars, |s| &mut s.hotbars),
    ];

    for (file_name, embedded, refresh, seen_slot) in collections {
        let path = global_dir.join(file_name);
        let Ok(user_text) = std::fs::read_to_string(&path) else {
            continue; // not extracted yet; extraction seeds via the next run
        };
        let seen = seen_slot(&mut sidecar);
        let outcome = match refresh(&user_text, embedded, seen.as_ref()) {
            Ok(outcome) => outcome,
            Err(err) => {
                // A user file that doesn't parse is their problem to fix;
                // never block startup or touch the file over it.
                tracing::warn!("defaults refresh skipped for {}: {}", file_name, err);
                continue;
            }
        };
        if let Some(updated) = outcome.updated_file {
            write_atomic(&path, updated)
                .with_context(|| format!("Failed to refresh {}", file_name))?;
            tracing::info!("Merged new shipped defaults into {}", file_name);
        }
        if seen.as_ref() != Some(&outcome.seen) {
            *seen = Some(outcome.seen);
            sidecar_dirty = true;
        }
    }

    // Managed data files: refresh while untouched, park once modified.
    for (file_name, embedded) in [
        ("cmdlist1.xml", super::DEFAULT_CMDLIST),
        ("spell_abbrev.toml", super::DEFAULT_SPELL_ABBREVS),
    ] {
        let path = global_dir.join(file_name);
        let Ok(current) = std::fs::read_to_string(&path) else {
            continue;
        };
        let current_hash = fnv1a(current.as_bytes());
        let embedded_hash = fnv1a(embedded.as_bytes());
        match sidecar.managed_hashes.get(file_name) {
            Some(recorded) if *recorded == current_hash => {
                if current_hash != embedded_hash {
                    write_atomic(&path, embedded)
                        .with_context(|| format!("Failed to refresh {}", file_name))?;
                    sidecar
                        .managed_hashes
                        .insert(file_name.to_string(), embedded_hash);
                    sidecar_dirty = true;
                    tracing::info!("Refreshed managed default file {}", file_name);
                }
            }
            Some(_) => {} // user modified it; parked permanently
            None => {
                // First sight: only start managing if it still matches the
                // embedded copy (i.e., the user never touched it).
                if current_hash == embedded_hash {
                    sidecar
                        .managed_hashes
                        .insert(file_name.to_string(), current_hash);
                    sidecar_dirty = true;
                }
            }
        }
    }

    if sidecar_dirty {
        let text = toml::to_string_pretty(&sidecar).context("sidecar serializes")?;
        write_atomic(&sidecar_path, text).context("Failed to write .defaults-seen.toml")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMBEDDED_HL: &str = "[alpha]\npattern = \"a\"\n\n[beta]\npattern = \"b\"\n";

    #[test]
    fn new_default_highlight_is_appended() {
        let user = "# mine\n[alpha]\npattern = \"custom\"\n";
        let seen: BTreeSet<String> = ["alpha".to_string()].into();
        let outcome = refresh_named_tables(user, EMBEDDED_HL, Some(&seen)).unwrap();
        let updated = outcome.updated_file.expect("beta should be added");
        assert!(updated.contains("[beta]"), "{}", updated);
        // The user's customized alpha is untouched, comments survive.
        assert!(updated.contains("pattern = \"custom\""), "{}", updated);
        assert!(updated.contains("# mine"), "{}", updated);
        assert!(outcome.seen.contains("beta"));
    }

    #[test]
    fn deleted_default_stays_deleted() {
        // beta was offered before (seen) and the user deleted it.
        let user = "[alpha]\npattern = \"a\"\n";
        let seen: BTreeSet<String> = ["alpha".to_string(), "beta".to_string()].into();
        let outcome = refresh_named_tables(user, EMBEDDED_HL, Some(&seen)).unwrap();
        assert!(outcome.updated_file.is_none());
    }

    #[test]
    fn seeding_pass_adds_nothing_and_records_union() {
        let user = "[mine]\npattern = \"m\"\n";
        let outcome = refresh_named_tables(user, EMBEDDED_HL, None).unwrap();
        assert!(outcome.updated_file.is_none());
        for key in ["alpha", "beta", "mine"] {
            assert!(outcome.seen.contains(key), "missing {}", key);
        }
    }

    #[test]
    fn user_keybind_additions_go_under_user_table() {
        let embedded = "[app]\nquit = \"ctrl+c\"\n\n[user]\nf5 = \"look\"\nf6 = \"health\"\n";
        let user = "[app]\nquit = \"ctrl+q\"\n\n[user]\nf5 = \"custom\"\n";
        let seen: BTreeSet<String> = ["f5".to_string()].into();
        let outcome = refresh_user_keybinds(user, embedded, Some(&seen)).unwrap();
        let updated = outcome.updated_file.expect("f6 should be added");
        assert!(updated.contains("f6 = \"health\""), "{}", updated);
        // User's own f5 override and their [app] section are untouched.
        assert!(updated.contains("f5 = \"custom\""), "{}", updated);
        assert!(updated.contains("quit = \"ctrl+q\""), "{}", updated);
    }

    #[test]
    fn controller_binds_refresh_into_their_own_table() {
        // A pre-controller user file gains the whole [controller] table;
        // its seen keys carry the controller/ prefix so they can't collide
        // with same-named [user] combos.
        let embedded =
            "[user]\nf5 = \"look\"\n\n[controller]\nstart = \"interact_mode\"\nsouth = \"look\"\n";
        let user = "[user]\nf5 = \"custom\"\n";
        let seen: BTreeSet<String> = ["f5".to_string()].into();
        let outcome = refresh_user_keybinds(user, embedded, Some(&seen)).unwrap();
        let updated = outcome.updated_file.expect("controller table should be added");
        assert!(updated.contains("[controller]"), "{}", updated);
        assert!(updated.contains("start = \"interact_mode\""), "{}", updated);
        assert!(updated.contains("f5 = \"custom\""), "{}", updated);
        assert!(outcome.seen.contains("controller/start"));
        assert!(outcome.seen.contains("controller/south"));
        assert!(outcome.seen.contains("f5"));
    }

    #[test]
    fn controller_shift_table_refreshes_with_prefixed_seen_keys() {
        let embedded = "[controller]\nstart = \"interact_mode\"\n\n[controller_shift]\nsouth = \"tts_stop\"\n";
        let user = "[controller]\nstart = \"interact_mode\"\n";
        let seen: BTreeSet<String> = ["controller/start".to_string()].into();
        let outcome = refresh_user_keybinds(user, embedded, Some(&seen)).unwrap();
        let updated = outcome.updated_file.expect("shift table should be added");
        assert!(updated.contains("[controller_shift]"), "{}", updated);
        assert!(outcome.seen.contains("controller_shift/south"));
    }

    #[test]
    fn deleted_controller_bind_stays_deleted() {
        let embedded = "[user]\nf5 = \"look\"\n\n[controller]\nstart = \"interact_mode\"\n";
        let user = "[user]\nf5 = \"look\"\n\n[controller]\n";
        let seen: BTreeSet<String> = ["f5".to_string(), "controller/start".to_string()].into();
        let outcome = refresh_user_keybinds(user, embedded, Some(&seen)).unwrap();
        assert!(
            outcome.updated_file.is_none(),
            "tombstoned controller bind must not resurrect"
        );
    }

    #[test]
    fn new_default_bar_is_appended_by_name() {
        let embedded = "[[bars]]\nname = \"default\"\n\n[[bars]]\nname = \"healing\"\n";
        let user = "[[bars]]\nname = \"default\"\n";
        let seen: BTreeSet<String> = ["default".to_string()].into();
        let outcome = refresh_bars(user, embedded, Some(&seen)).unwrap();
        let updated = outcome.updated_file.expect("healing bar should be added");
        assert!(updated.contains("name = \"healing\""), "{}", updated);
    }

    #[test]
    fn fnv1a_is_stable() {
        assert_eq!(fnv1a(b""), "cbf29ce484222325");
        assert_eq!(fnv1a(b"a"), "af63dc4c8601ec8c");
    }
}
