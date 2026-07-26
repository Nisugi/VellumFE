//! Remote config editing: the web client's settings sheet reads and writes
//! a whitelisted set of config files (import/export on devices where the
//! filesystem isn't reachable — the Android app's private storage).
//!
//! Writes are validated by parsing into the real config types before
//! touching disk, then hot-reloaded so highlights/colors apply live.
//! Replies are addressed to the requesting client (Menu pattern).

use super::state::AppCore;
use crate::config::registry::{self, SettingKind, SettingScope, SettingValue};
use crate::config::Config;

// ---- Registry settings dump (phone settings sheet) -------------------------
// Pure builders kept outside the registry itself: the registry stays a
// plain table, the wire shape lives with the wire handlers.

/// One `SettingValue` as wire JSON (Bool → bool, Int/Float → number,
/// Text → string, List → array of strings).
fn setting_value_json(value: &SettingValue) -> serde_json::Value {
    match value {
        SettingValue::Bool(v) => serde_json::json!(v),
        SettingValue::Int(v) => serde_json::json!(v),
        SettingValue::Float(v) => serde_json::json!(v),
        SettingValue::Text(v) => serde_json::json!(v),
        SettingValue::List(v) => serde_json::json!(v),
    }
}

fn setting_kind_json(kind: &SettingKind) -> serde_json::Value {
    match kind {
        SettingKind::Bool => serde_json::json!({ "type": "bool" }),
        SettingKind::Int { min, max } => {
            serde_json::json!({ "type": "int", "min": min, "max": max })
        }
        SettingKind::Float { min, max } => {
            serde_json::json!({ "type": "float", "min": min, "max": max })
        }
        SettingKind::Text => serde_json::json!({ "type": "text" }),
        SettingKind::OptionalText => serde_json::json!({ "type": "optional_text" }),
        SettingKind::Enum { options } => {
            serde_json::json!({ "type": "enum", "options": options })
        }
        SettingKind::List => serde_json::json!({ "type": "list" }),
    }
}

/// The full settings catalog as an ordered JSON array (registry order,
/// i.e. sorted by key), with live values from `config`. Sensitive
/// settings never ship their value: they send `""` plus
/// `redacted: true` when a value exists server-side.
pub(crate) fn settings_catalog_json(config: &Config) -> serde_json::Value {
    let entries: Vec<serde_json::Value> = registry::registry()
        .iter()
        .map(|def| {
            let live = (def.get)(config);
            let (value, redacted) = if def.sensitive {
                let has_value = match &live {
                    SettingValue::Text(s) => !s.is_empty(),
                    SettingValue::List(l) => !l.is_empty(),
                    _ => true,
                };
                (serde_json::json!(""), has_value)
            } else {
                (setting_value_json(&live), false)
            };
            let mut entry = serde_json::json!({
                "key": def.key,
                "label": def.label,
                "category": def.category,
                "description": def.description,
                "kind": setting_kind_json(&def.kind),
                "scope": match def.scope {
                    SettingScope::GlobalOrCharacter => "global_or_character",
                    SettingScope::CharacterOnly => "character_only",
                },
                "sensitive": def.sensitive,
                "value": value,
            });
            if redacted {
                entry["redacted"] = serde_json::json!(true);
            }
            entry
        })
        .collect();
    serde_json::Value::Array(entries)
}

/// Convert a wire JSON value into the `SettingValue` a kind expects.
/// Range/option validation stays in the registry setters; this only
/// bridges JSON typing.
pub(crate) fn setting_value_from_json(
    kind: &SettingKind,
    value: &serde_json::Value,
) -> Result<SettingValue, String> {
    match kind {
        SettingKind::Bool => value
            .as_bool()
            .map(SettingValue::Bool)
            .ok_or_else(|| "expected a boolean".to_string()),
        SettingKind::Int { .. } => value
            .as_i64()
            .map(SettingValue::Int)
            .ok_or_else(|| "expected an integer".to_string()),
        SettingKind::Float { .. } => value
            .as_f64()
            .map(SettingValue::Float)
            .ok_or_else(|| "expected a number".to_string()),
        SettingKind::Text | SettingKind::OptionalText | SettingKind::Enum { .. } => value
            .as_str()
            .map(|s| SettingValue::Text(s.to_string()))
            .ok_or_else(|| "expected text".to_string()),
        SettingKind::List => {
            let items = value.as_array().ok_or("expected a list")?;
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "list entries must be text".to_string())
                })
                .collect::<Result<Vec<String>, String>>()
                .map(SettingValue::List)
        }
    }
}

impl AppCore {
    /// Map a whitelisted editor file id to its path. Client-supplied paths
    /// are never accepted — ids only.
    fn config_editor_path(&self, file: &str) -> Result<std::path::PathBuf, String> {
        let character = self.config.character.as_deref();
        match file {
            "highlights" => Config::highlights_path(character),
            "highlights-global" => Config::common_highlights_path(),
            "colors" => Config::colors_path(character),
            "colors-global" => Config::common_colors_path(),
            _ => return Err(format!("Unknown config file '{file}'")),
        }
        .map_err(|e| format!("Path unavailable: {e}"))
    }

    /// Parse into the real config type so a bad save can't wedge startup.
    fn validate_config_content(file: &str, content: &str) -> Result<(), String> {
        let parse_error = match file {
            "highlights" | "highlights-global" => toml::from_str::<
                std::collections::HashMap<String, crate::config::HighlightPattern>,
            >(content)
            .err()
            .map(|e| e.to_string()),
            "colors" | "colors-global" => toml::from_str::<crate::config::ColorConfig>(content)
                .err()
                .map(|e| e.to_string()),
            _ => return Err(format!("Unknown config file '{file}'")),
        };
        match parse_error {
            None => Ok(()),
            Some(e) => Err(format!("Invalid TOML: {e}")),
        }
    }

    /// Read a config file for a remote client. A missing file reads as
    /// empty (both files are optional on disk).
    pub fn handle_remote_config_get(&mut self, client_id: u64, request_id: u64, file: String) {
        let result = self.config_editor_path(&file).and_then(|path| {
            if path.exists() {
                std::fs::read_to_string(&path).map_err(|e| format!("Read failed: {e}"))
            } else {
                Ok(String::new())
            }
        });
        let (content, error) = match result {
            Ok(content) => (Some(content), None),
            Err(e) => (None, Some(e)),
        };
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_config_file(client_id, request_id, file, content, error, false);
        }
    }

    // ---- Structured highlight editing (phone editor UI) ----------------
    // Operates on one scope's file (profile or global), never the merged
    // view. Rewriting from the parsed map drops comments/ordering in the
    // file — order was already non-semantic at runtime (HashMap).

    fn highlights_scope_path(&self, scope: &str) -> Result<std::path::PathBuf, String> {
        match scope {
            "profile" => Config::highlights_path(self.config.character.as_deref()),
            "global" => Config::common_highlights_path(),
            _ => return Err(format!("Unknown highlights scope '{scope}'")),
        }
        .map_err(|e| format!("Path unavailable: {e}"))
    }

    fn load_highlights_map(
        path: &std::path::Path,
    ) -> Result<std::collections::HashMap<String, crate::config::HighlightPattern>, String> {
        if !path.exists() {
            return Ok(Default::default());
        }
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Read failed: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("Existing file is invalid TOML: {e}"))
    }

    fn save_highlights_map(
        path: &std::path::Path,
        map: &std::collections::HashMap<String, crate::config::HighlightPattern>,
    ) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content =
            toml::to_string_pretty(map).map_err(|e| format!("Serialize failed: {e}"))?;
        crate::config::write_atomic(path, content).map_err(|e| format!("Write failed: {e}"))
    }

    /// Sound files available for highlight rules (the form's dropdown).
    fn list_sound_files() -> Vec<String> {
        let Ok(dir) = Config::sounds_dir() else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let lower = name.to_lowercase();
                [".mp3", ".wav", ".ogg", ".flac"]
                    .iter()
                    .any(|ext| lower.ends_with(ext))
                    .then_some(name)
            })
            .collect();
        names.sort();
        names
    }

    fn reply_highlights(
        &mut self,
        client_id: u64,
        request_id: u64,
        scope: String,
        result: Result<std::collections::HashMap<String, crate::config::HighlightPattern>, String>,
    ) {
        let (rules, error) = match result {
            Ok(map) => (
                serde_json::to_value(&map).unwrap_or(serde_json::Value::Null),
                None,
            ),
            Err(e) => (serde_json::Value::Null, Some(e)),
        };
        let sounds = Self::list_sound_files();
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_highlights(client_id, request_id, scope, rules, sounds, error);
        }
    }

    pub fn handle_remote_highlights_get(
        &mut self,
        client_id: u64,
        request_id: u64,
        scope: String,
    ) {
        let result = self
            .highlights_scope_path(&scope)
            .and_then(|path| Self::load_highlights_map(&path));
        self.reply_highlights(client_id, request_id, scope, result);
    }

    pub fn handle_remote_highlight_put(
        &mut self,
        client_id: u64,
        request_id: u64,
        scope: String,
        name: String,
        rule: serde_json::Value,
    ) {
        let result = (|| {
            if name.trim().is_empty() {
                return Err("Rule name is required".to_string());
            }
            let rule: crate::config::HighlightPattern =
                serde_json::from_value(rule).map_err(|e| format!("Invalid rule: {e}"))?;
            if rule.pattern.trim().is_empty() {
                return Err("Pattern is required".to_string());
            }
            // fast_parse patterns are literal alternations; everything else
            // must compile as a regex or the engine will reject it later.
            if !rule.fast_parse {
                regex::Regex::new(&rule.pattern).map_err(|e| format!("Bad regex: {e}"))?;
            }
            let path = self.highlights_scope_path(&scope)?;
            let mut map = Self::load_highlights_map(&path)?;
            map.insert(name.trim().to_string(), rule);
            Self::save_highlights_map(&path, &map)?;
            Ok(map)
        })();
        let ok = result.is_ok();
        self.reply_highlights(client_id, request_id, scope, result);
        if ok {
            self.reload_highlights();
        }
    }

    pub fn handle_remote_highlight_delete(
        &mut self,
        client_id: u64,
        request_id: u64,
        scope: String,
        name: String,
    ) {
        let result = (|| {
            let path = self.highlights_scope_path(&scope)?;
            let mut map = Self::load_highlights_map(&path)?;
            if map.remove(&name).is_none() {
                return Err(format!("No rule named '{name}'"));
            }
            Self::save_highlights_map(&path, &map)?;
            Ok(map)
        })();
        let ok = result.is_ok();
        self.reply_highlights(client_id, request_id, scope, result);
        if ok {
            self.reload_highlights();
        }
    }

    // ---- Structured colors editing (phone editor UI) --------------------

    fn colors_scope_path(&self, scope: &str) -> Result<std::path::PathBuf, String> {
        match scope {
            "profile" => Config::colors_path(self.config.character.as_deref()),
            "global" => Config::common_colors_path(),
            _ => return Err(format!("Unknown colors scope '{scope}'")),
        }
        .map_err(|e| format!("Path unavailable: {e}"))
    }

    /// Read the color config for editing. A missing file shows the
    /// *effective* config (merged for profile scope, defaults for global)
    /// so the editor lists real preset names rather than an empty page.
    pub fn handle_remote_colors_get(&mut self, client_id: u64, request_id: u64, scope: String) {
        let result = self.colors_scope_path(&scope).and_then(|path| {
            let config = if path.exists() {
                let content =
                    std::fs::read_to_string(&path).map_err(|e| format!("Read failed: {e}"))?;
                toml::from_str::<crate::config::ColorConfig>(&content)
                    .map_err(|e| format!("Existing file is invalid TOML: {e}"))?
            } else if scope == "profile" {
                crate::config::ColorConfig::load(self.config.character.as_deref())
                    .unwrap_or_default()
            } else {
                crate::config::ColorConfig::default()
            };
            serde_json::to_value(&config).map_err(|e| format!("Serialize failed: {e}"))
        });
        let (colors, error) = match result {
            Ok(v) => (v, None),
            Err(e) => (serde_json::Value::Null, Some(e)),
        };
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_colors(client_id, request_id, scope, colors, error, false);
        }
    }

    /// Validate (deserializes into the real ColorConfig), write, hot-reload.
    pub fn handle_remote_colors_put(
        &mut self,
        client_id: u64,
        request_id: u64,
        scope: String,
        colors: serde_json::Value,
    ) {
        let result = (|| {
            let config: crate::config::ColorConfig = serde_json::from_value(colors)
                .map_err(|e| format!("Invalid color config: {e}"))?;
            let path = self.colors_scope_path(&scope)?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let content =
                toml::to_string_pretty(&config).map_err(|e| format!("Serialize failed: {e}"))?;
            crate::config::write_atomic(&path, content).map_err(|e| format!("Write failed: {e}"))
        })();
        let (saved, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        if saved {
            self.reload_colors();
        }
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_colors(
                client_id,
                request_id,
                scope,
                serde_json::Value::Null,
                error,
                saved,
            );
        }
    }

    // ---- Registry settings (phone settings sheet) -----------------------

    /// Send the full settings catalog (registry dump + live values) to the
    /// requesting client. Sensitive values are redacted by the builder.
    pub fn handle_remote_settings_get(&mut self, client_id: u64, request_id: u64) {
        let catalog = settings_catalog_json(&self.config);
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_settings(client_id, request_id, catalog, None, None, false);
        }
    }

    /// Apply one registered setting to the live config, persist it sparsely
    /// to the requested scope, and hot-refresh the message processor.
    /// `scope` is "character" or "global" (character-only settings are
    /// forced to the character file by `save_single_setting`).
    pub fn handle_remote_settings_put(
        &mut self,
        client_id: u64,
        request_id: u64,
        key: String,
        value: serde_json::Value,
        scope: String,
    ) {
        let result = (|| {
            if !matches!(scope.as_str(), "character" | "global") {
                return Err(format!("Unknown settings scope '{scope}'"));
            }
            let def = registry::find(&key).ok_or_else(|| format!("Unknown setting '{key}'"))?;
            let value = setting_value_from_json(&def.kind, &value)?;
            (def.set)(&mut self.config, &value)?;
            let character = self.config.character.clone();
            self.config
                .save_single_setting(&key, scope == "global", character.as_deref())
                .map_err(|e| format!("Save failed: {e}"))?;
            Ok(())
        })();
        let (saved, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        if saved {
            // Hot-refresh what the server side can: the message pipeline
            // (stream routing, echo, highlight toggles, squelch/redirect
            // caches) re-reads the config here. Frontend-owned systems
            // (theme, sound/TTS engines, web server bind) pick the change
            // up on their own schedule or at restart.
            self.message_processor.apply_config(self.config.clone());
            self.needs_render = true;
        }
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_settings(
                client_id,
                request_id,
                serde_json::Value::Null,
                Some(key),
                error,
                saved,
            );
        }
    }

    /// Validate, write, and hot-reload a config file for a remote client.
    pub fn handle_remote_config_put(
        &mut self,
        client_id: u64,
        request_id: u64,
        file: String,
        content: String,
    ) {
        let result = Self::validate_config_content(&file, &content)
            .and_then(|()| self.config_editor_path(&file))
            .and_then(|path| {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                crate::config::write_atomic(&path, &content).map_err(|e| format!("Write failed: {e}"))
            });
        let (saved, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        if saved {
            match file.as_str() {
                "highlights" | "highlights-global" => self.reload_highlights(),
                "colors" | "colors-global" => self.reload_colors(),
                _ => {}
            }
        }
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_config_file(client_id, request_id, file, None, error, saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registered key appears in the catalog exactly once, in
    /// registry order, with the full wire shape.
    #[test]
    fn catalog_covers_every_registered_key_once() {
        let config = Config::default();
        let catalog = settings_catalog_json(&config);
        let entries = catalog.as_array().expect("catalog is an array");
        assert_eq!(entries.len(), registry::registry().len());
        for (entry, def) in entries.iter().zip(registry::registry()) {
            assert_eq!(entry["key"], def.key, "catalog order matches registry");
            for field in ["label", "category", "description", "kind", "scope", "sensitive"] {
                assert!(
                    !entry[field].is_null(),
                    "{} entry is missing '{}'",
                    def.key,
                    field
                );
            }
            assert!(entry["kind"]["type"].is_string(), "{} kind has a type", def.key);
            assert!(
                entry.get("value").is_some(),
                "{} entry is missing 'value'",
                def.key
            );
        }
        // Exactly once: no duplicate keys.
        let mut keys: Vec<&str> = entries
            .iter()
            .map(|e| e["key"].as_str().unwrap())
            .collect();
        keys.dedup();
        assert_eq!(keys.len(), entries.len(), "no duplicate catalog keys");
    }

    /// Sensitive settings never ship their value — `""` plus a redacted
    /// flag when a value exists.
    #[test]
    fn catalog_redacts_sensitive_values() {
        let mut config = Config::default();
        config.connection.password = Some("hunter2secret".to_string());
        config.connection.account = Some("MYACCOUNT".to_string());
        let catalog = settings_catalog_json(&config);
        let entry = catalog
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["key"] == "connection.password")
            .expect("password is registered");
        assert_eq!(entry["sensitive"], true);
        assert_eq!(entry["value"], "");
        assert_eq!(entry["redacted"], true);
        // The secret never appears anywhere in the serialized catalog.
        assert!(!catalog.to_string().contains("hunter2secret"));
        // Non-sensitive optional text still ships its value.
        let account = catalog
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["key"] == "connection.account")
            .unwrap();
        assert_eq!(account["value"], "MYACCOUNT");

        // No stored password: still "" but not flagged redacted.
        let empty = settings_catalog_json(&Config::default());
        let entry = empty
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["key"] == "connection.password")
            .unwrap();
        assert_eq!(entry["value"], "");
        assert!(entry.get("redacted").is_none());
    }

    /// JSON → SettingValue conversion: every kind converts its own JSON
    /// type and rejects the others.
    #[test]
    fn value_conversion_per_kind() {
        use serde_json::json;
        let int_kind = SettingKind::Int { min: 0, max: 10 };
        let float_kind = SettingKind::Float { min: 0.0, max: 1.0 };
        let enum_kind = SettingKind::Enum { options: &["a", "b"] };

        assert_eq!(
            setting_value_from_json(&SettingKind::Bool, &json!(true)),
            Ok(SettingValue::Bool(true))
        );
        assert_eq!(
            setting_value_from_json(&int_kind, &json!(7)),
            Ok(SettingValue::Int(7))
        );
        assert_eq!(
            setting_value_from_json(&float_kind, &json!(0.5)),
            Ok(SettingValue::Float(0.5))
        );
        // Floats also accept whole JSON numbers.
        assert_eq!(
            setting_value_from_json(&float_kind, &json!(1)),
            Ok(SettingValue::Float(1.0))
        );
        assert_eq!(
            setting_value_from_json(&SettingKind::Text, &json!("hi")),
            Ok(SettingValue::Text("hi".to_string()))
        );
        assert_eq!(
            setting_value_from_json(&SettingKind::OptionalText, &json!("")),
            Ok(SettingValue::Text(String::new()))
        );
        assert_eq!(
            setting_value_from_json(&enum_kind, &json!("b")),
            Ok(SettingValue::Text("b".to_string()))
        );
        assert_eq!(
            setting_value_from_json(&SettingKind::List, &json!(["x", "y"])),
            Ok(SettingValue::List(vec!["x".to_string(), "y".to_string()]))
        );

        // Rejection paths.
        assert!(setting_value_from_json(&SettingKind::Bool, &json!(1)).is_err());
        assert!(setting_value_from_json(&int_kind, &json!(1.5)).is_err());
        assert!(setting_value_from_json(&int_kind, &json!("7")).is_err());
        assert!(setting_value_from_json(&float_kind, &json!("0.5")).is_err());
        assert!(setting_value_from_json(&SettingKind::Text, &json!(3)).is_err());
        assert!(setting_value_from_json(&enum_kind, &json!(null)).is_err());
        assert!(setting_value_from_json(&SettingKind::List, &json!("x")).is_err());
        assert!(setting_value_from_json(&SettingKind::List, &json!(["x", 3])).is_err());
    }

    /// The wire can express every registered setting: value → JSON →
    /// SettingValue survives for the whole catalog.
    #[test]
    fn every_setting_round_trips_through_wire_json() {
        let config = Config::default();
        for def in registry::registry() {
            let live = (def.get)(&config);
            let wire = setting_value_json(&live);
            let back = setting_value_from_json(&def.kind, &wire)
                .unwrap_or_else(|e| panic!("{} failed wire round trip: {}", def.key, e));
            match (&live, &back) {
                (SettingValue::Float(a), SettingValue::Float(b)) => {
                    assert!((a - b).abs() < 1e-9, "{} float drifted", def.key)
                }
                _ => assert_eq!(&back, &live, "{} changed across the wire", def.key),
            }
        }
    }
}
