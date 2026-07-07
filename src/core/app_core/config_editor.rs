//! Remote config editing: the web client's settings sheet reads and writes
//! a whitelisted set of config files (import/export on devices where the
//! filesystem isn't reachable — the Android app's private storage).
//!
//! Writes are validated by parsing into the real config types before
//! touching disk, then hot-reloaded so highlights/colors apply live.
//! Replies are addressed to the requesting client (Menu pattern).

use super::state::AppCore;
use crate::config::Config;

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
        std::fs::write(path, content).map_err(|e| format!("Write failed: {e}"))
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
                std::fs::write(&path, &content).map_err(|e| format!("Write failed: {e}"))
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
