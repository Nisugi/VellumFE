//! Stream-routing mutations shared by every frontend's Streams surface
//! (the GUI's Streams & Custom Windows panel and the TUI's `.streams`
//! menu). These lived as near-verbatim copies in both frontends until
//! the dot-command parity audit (D4); the copies had already drifted —
//! the GUI accepted Text/Inventory/Reserve/Spells windows as
//! subscription targets while the TUI accepted only plain Text. Core
//! keeps the union (the broader accept).

use super::AppCore;
use crate::config::StreamRoute;
use crate::data::{TextContent, WindowContent};

/// The text-bearing widget contents a stream can be subscribed to.
fn text_content_mut(content: &mut WindowContent) -> Option<&mut TextContent> {
    match content {
        WindowContent::Text(text)
        | WindowContent::Inventory(text)
        | WindowContent::Reserve(text)
        | WindowContent::Spells(text) => Some(text),
        _ => None,
    }
}

impl AppCore {
    /// Mirror a text window's live stream list (and optionally buffer
    /// size) into its layout definition so the change survives a restart
    /// (same fix the Window Editor carries).
    pub fn sync_text_streams_to_layout(
        &mut self,
        name: &str,
        streams: Vec<String>,
        buffer_size: Option<usize>,
    ) {
        if let Some(crate::config::WindowDef::Text { data, .. }) =
            self.layout.windows.iter_mut().find(|w| w.name() == name)
        {
            data.streams = streams;
            if let Some(buffer_size) = buffer_size {
                data.buffer_size = buffer_size;
            }
            self.schedule_layout_autosave();
        }
    }

    /// Remove `stream` from every plain Text window's stream list except
    /// `keep`, syncing layout definitions and the routing cache. Tabbed
    /// tabs and built-in inventory-type widgets are Window Editor
    /// territory and are left alone.
    pub fn remove_stream_from_text_windows(&mut self, stream: &str, keep: Option<&str>) {
        let mut changed: Vec<(String, Vec<String>)> = Vec::new();
        for (name, window) in self.ui_state.windows.iter_mut() {
            if keep == Some(name.as_str()) {
                continue;
            }
            let WindowContent::Text(text) = &mut window.content else {
                continue;
            };
            let before = text.streams.len();
            text.streams
                .retain(|s| !s.trim().eq_ignore_ascii_case(stream));
            if text.streams.len() != before {
                changed.push((name.clone(), text.streams.clone()));
            }
        }
        if changed.is_empty() {
            return;
        }
        for (name, streams) in changed {
            self.sync_text_streams_to_layout(&name, streams, None);
        }
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
        self.needs_render = true;
    }

    /// Subscribe an existing text-bearing window to `stream` (no-op when
    /// already subscribed), syncing the layout definition and the routing
    /// cache.
    pub fn add_stream_to_text_window(&mut self, name: &str, stream: &str) -> Result<(), String> {
        let streams = {
            let window = self
                .ui_state
                .windows
                .get_mut(name)
                .ok_or_else(|| format!("Window '{}' no longer exists.", name))?;
            let text = text_content_mut(&mut window.content)
                .ok_or_else(|| format!("Window '{}' is not a text window.", name))?;
            if !text
                .streams
                .iter()
                .any(|s| s.trim().eq_ignore_ascii_case(stream))
            {
                text.streams.push(stream.to_string());
            }
            text.streams.clone()
        };
        self.sync_text_streams_to_layout(name, streams, None);
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
        self.needs_render = true;
        Ok(())
    }

    /// Set (or clear, with `None`) the `[streams.routes]` entry for a
    /// stream, persist it via the sparse profile save, and push the new
    /// config into the message processor (which routes from its own
    /// copy).
    pub fn set_stream_route(
        &mut self,
        stream: &str,
        route: Option<StreamRoute>,
    ) -> Result<(), String> {
        let routes = &mut self.config.streams.routes;
        // Replace any existing entry regardless of letter case (route
        // lookup is case-insensitive, so near-duplicates would shadow
        // each other).
        routes.retain(|id, _| !id.eq_ignore_ascii_case(stream));
        if let Some(route) = route {
            routes.insert(stream.to_string(), route);
        }
        self.save_config()
            .map_err(|err| format!("Failed to save config: {}", err))?;
        let config = self.config.clone();
        self.message_processor.apply_config(config);
        self.needs_render = true;
        Ok(())
    }
}
