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
