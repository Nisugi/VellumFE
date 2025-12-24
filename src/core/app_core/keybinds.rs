use std::collections::HashMap;

use anyhow::Result;

use crate::config::{BorderSides, Config, KeyAction, KeyBindAction, PerformanceWidgetData, WindowBase, WindowDef};
use crate::frontend::common::KeyEvent;

use super::AppCore;

impl AppCore {
    /// Build runtime keybind map from config for fast O(1) lookups
    /// Converts string-based keybinds (e.g., "num_0", "Ctrl+s") to KeyEvent structs
    pub(super) fn build_keybind_map(config: &Config) -> HashMap<KeyEvent, KeyBindAction> {
        let mut map = HashMap::new();

        for (key_string, action) in &config.keybinds {
            // Parse the key string into a (KeyCode, KeyModifiers) tuple
            if let Some((code, modifiers)) = crate::config::parse_key_string(key_string) {
                // Create a KeyEvent from the parsed code and modifiers
                let key_event = KeyEvent { code, modifiers };
                map.insert(key_event, action.clone());
            } else {
                tracing::warn!("Failed to parse keybind string: '{}'", key_string);
            }
        }

        tracing::debug!("Built keybind map with {} entries", map.len());
        map
    }

    /// Rebuild the keybind map (call after config changes)
    pub fn rebuild_keybind_map(&mut self) {
        self.keybind_map = Self::build_keybind_map(&self.config);
    }

    /// Execute a keybind action (called when a bound key is pressed)
    /// Returns a list of commands to send to the server (for macros)
    pub fn execute_keybind_action(&mut self, action: &KeyBindAction) -> Result<Vec<String>> {
        match action {
            KeyBindAction::Action(action_str) => {
                // Parse the action string to a KeyAction
                if let Some(key_action) = KeyAction::from_str(action_str) {
                    self.execute_key_action(key_action)?;
                } else {
                    tracing::warn!("Unknown keybind action: '{}'", action_str);
                }
                Ok(vec![]) // Actions don't send commands to server
            }
            KeyBindAction::Macro(macro_action) => {
                // Strip any trailing \r or \n from macro text (legacy from wrayth-style macros)
                // These control characters corrupt the StyledLine and cause rendering artifacts
                let clean_text =
                    macro_action.macro_text.trim_end_matches(&['\r', '\n'][..]).to_string();

                tracing::info!(
                    "[MACRO] Executing macro: '{}' (raw: '{}')",
                    clean_text,
                    macro_action.macro_text
                );

                // Send the macro text as a command (posts prompt+echo, returns command for server)
                let command = self.send_command(clean_text)?;
                tracing::info!("[MACRO] send_command returned: '{}'", command);
                Ok(vec![command]) // Return command for network layer to send
            }
        }
    }

    /// Execute a KeyAction (dispatch to the appropriate method)
    fn execute_key_action(&mut self, action: KeyAction) -> Result<()> {
        match action {
            // Command input actions - now handled by CommandInput widget
            KeyAction::SendCommand
            | KeyAction::CursorLeft
            | KeyAction::CursorRight
            | KeyAction::CursorWordLeft
            | KeyAction::CursorWordRight
            | KeyAction::CursorHome
            | KeyAction::CursorEnd
            | KeyAction::CursorBackspace
            | KeyAction::CursorDelete
            | KeyAction::CursorDeleteWord
            | KeyAction::CursorClearLine
            | KeyAction::PreviousCommand
            | KeyAction::NextCommand
            | KeyAction::SendLastCommand
            | KeyAction::SendSecondLastCommand
            | KeyAction::Copy
            | KeyAction::Paste
            | KeyAction::SelectAll => {
                // These actions are now handled by the CommandInput widget
                // via frontend.command_input_key() in main.rs
                // If we get here, it means the routing logic in main.rs missed something
                tracing::warn!(
                    "Command input action {:?} reached execute_key_action - should be routed to widget",
                    action
                );
            }

            // Window actions
            KeyAction::SwitchCurrentWindow => {
                // Handled in input_handlers.rs for smart Tab completion
                tracing::debug!("SwitchCurrentWindow reached keybinds.rs - should be handled in input_handlers");
            }
            KeyAction::ScrollCurrentWindowUpOne => {
                tracing::debug!("KeyAction::ScrollCurrentWindowUpOne triggered");
                self.scroll_current_window_up_one();
            }
            KeyAction::ScrollCurrentWindowDownOne => {
                tracing::debug!("KeyAction::ScrollCurrentWindowDownOne triggered");
                self.scroll_current_window_down_one();
            }
            KeyAction::ScrollCurrentWindowUpPage => {
                tracing::debug!("KeyAction::ScrollCurrentWindowUpPage triggered");
                self.scroll_current_window_up_page();
            }
            KeyAction::ScrollCurrentWindowDownPage => {
                tracing::debug!("KeyAction::ScrollCurrentWindowDownPage triggered");
                self.scroll_current_window_down_page();
            }
            KeyAction::ScrollCurrentWindowHome => {
                tracing::debug!("KeyAction::ScrollCurrentWindowHome triggered");
                self.scroll_current_window_home();
            }
            KeyAction::ScrollCurrentWindowEnd => {
                tracing::debug!("KeyAction::ScrollCurrentWindowEnd triggered");
                self.scroll_current_window_end();
            }

            // Search actions - handled in frontend layer (TuiFrontend.handle_normal_mode_keys)
            // These require frontend access to manipulate text windows
            KeyAction::StartSearch => {
                tracing::debug!("StartSearch handled in frontend layer");
            }
            KeyAction::NextSearchMatch => {
                tracing::debug!("NextSearchMatch handled in frontend layer");
            }
            KeyAction::PrevSearchMatch => {
                tracing::debug!("PrevSearchMatch handled in frontend layer");
            }
            KeyAction::ClearSearch => {
                tracing::debug!("ClearSearch handled in frontend layer");
            }

            // Tab navigation actions - need to be handled in main.rs (require frontend access)
            KeyAction::NextTab | KeyAction::PrevTab | KeyAction::NextUnreadTab => {
                // These actions must be routed to frontend in main.rs
                // execute_key_action doesn't have frontend access
                tracing::warn!(
                    "Tab navigation action {:?} reached execute_key_action - should be routed to frontend",
                    action
                );
            }

            // System toggles
            KeyAction::TogglePerformanceStats => {
                let enabled = self.toggle_performance_overlay();
                let status = if enabled { "enabled" } else { "disabled" };
                self.add_system_message(&format!("Performance overlay {}", status));
                tracing::info!("Performance stats overlay toggled: {}", status);
            }
            KeyAction::ToggleSounds => {
                self.config.sound.enabled = !self.config.sound.enabled;
                let status = if self.config.sound.enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                self.add_system_message(&format!("Sound system {}", status));
                tracing::info!("Sound system toggled: {}", status);
            }

            // TTS (Text-to-Speech) actions - Accessibility
            KeyAction::TtsNext => {
                if let Err(e) = self.tts_manager.speak_next() {
                    tracing::warn!("TTS speak_next failed: {}", e);
                }
            }
            KeyAction::TtsPrevious => {
                if let Err(e) = self.tts_manager.speak_previous() {
                    tracing::warn!("TTS speak_previous failed: {}", e);
                }
            }
            KeyAction::TtsNextUnread => {
                if let Err(e) = self.tts_manager.speak_next_unread() {
                    tracing::warn!("TTS speak_next_unread failed: {}", e);
                }
            }
            KeyAction::TtsStop => {
                if let Err(e) = self.tts_manager.stop() {
                    tracing::warn!("TTS stop failed: {}", e);
                }
            }
            KeyAction::TtsMuteToggle => {
                self.tts_manager.toggle_mute();
                let status = if self.tts_manager.is_muted() { "muted" } else { "unmuted" };
                self.add_system_message(&format!("TTS {}", status));
            }
            KeyAction::TtsIncreaseRate => {
                if let Err(e) = self.tts_manager.increase_rate() {
                    tracing::warn!("TTS increase_rate failed: {}", e);
                } else {
                    self.add_system_message("TTS rate increased");
                }
            }
            KeyAction::TtsDecreaseRate => {
                if let Err(e) = self.tts_manager.decrease_rate() {
                    tracing::warn!("TTS decrease_rate failed: {}", e);
                } else {
                    self.add_system_message("TTS rate decreased");
                }
            }
            KeyAction::TtsIncreaseVolume => {
                if let Err(e) = self.tts_manager.increase_volume() {
                    tracing::warn!("TTS increase_volume failed: {}", e);
                } else {
                    self.add_system_message("TTS volume increased");
                }
            }
            KeyAction::TtsDecreaseVolume => {
                if let Err(e) = self.tts_manager.decrease_volume() {
                    tracing::warn!("TTS decrease_volume failed: {}", e);
                } else {
                    self.add_system_message("TTS volume decreased");
                }
            }

            // Macro actions (should not reach here - handled by execute_keybind_action)
            KeyAction::SendMacro(text) => {
                self.send_command(text)?;
            }
        }

        Ok(())
    }

    /// Toggle the performance overlay window using the performance template
    /// Returns the new enabled state
    fn toggle_performance_overlay(&mut self) -> bool {
        const OVERLAY_NAME: &str = "performance_overlay";

        // If it's already present, remove it and disable collection
        if self.ui_state.remove_window(OVERLAY_NAME).is_some() {
            let data = self.perf_overlay_data(false);
            self.perf_stats.apply_enabled_from(&data);
            self.config.ui.performance_stats_enabled = false;
            self.needs_render = true;
            return false;
        }

        // Build window def from template and override geometry from UI config
        let mut window_def = self.build_perf_overlay_def();
        window_def.base_mut().name = OVERLAY_NAME.to_string();
        window_def.base_mut().row = self.config.ui.perf_stats_y;
        window_def.base_mut().col = self.config.ui.perf_stats_x;
        window_def.base_mut().rows = self.config.ui.perf_stats_height.max(1);
        window_def.base_mut().cols = self.config.ui.perf_stats_width.max(1);

        // Add to UI state only (does not touch layout)
        self.add_new_window(&window_def, 0, 0);

        // Enable collection based on template data
        let data = self.perf_overlay_data(true);
        self.perf_stats.apply_enabled_from(&data);
        self.config.ui.performance_stats_enabled = true;
        self.needs_render = true;
        true
    }

    fn perf_overlay_data(&self, enabled: bool) -> PerformanceWidgetData {
        if let Some(WindowDef::Performance { data, .. }) =
            Config::get_window_template("performance")
        {
            let mut data = data.clone();
            data.enabled = enabled;
            return data;
        }

        PerformanceWidgetData {
            enabled,
            show_fps: true,
            show_frame_times: false,
            show_render_times: true,
            show_ui_times: true,
            show_wrap_times: true,
            show_net: true,
            show_parse: true,
            show_events: true,
            show_memory: true,
            show_lines: true,
            show_uptime: true,
            show_jitter: false,
            show_frame_spikes: false,
            show_event_lag: false,
            show_memory_delta: true,
        }
    }

    fn build_perf_overlay_def(&self) -> WindowDef {
        if let Some(WindowDef::Performance { base, data }) =
            Config::get_window_template("performance")
        {
            return WindowDef::Performance { base, data };
        }

        let base = WindowBase {
            name: "performance".to_string(),
            row: self.config.ui.perf_stats_y,
            col: self.config.ui.perf_stats_x,
            rows: self.config.ui.perf_stats_height.max(1),
            cols: self.config.ui.perf_stats_width.max(1),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some("Performance Stats".to_string()),
            title_position: "top-left".to_string(),
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            visible: true,
            content_align: None,
        };

        WindowDef::Performance {
            base,
            data: PerformanceWidgetData {
                enabled: true,
                show_fps: true,
                show_frame_times: true,
                show_render_times: true,
                show_ui_times: true,
                show_wrap_times: true,
                show_net: true,
                show_parse: true,
                show_events: true,
                show_memory: true,
                show_lines: true,
                show_uptime: true,
                show_jitter: true,
                show_frame_spikes: true,
                show_event_lag: true,
                show_memory_delta: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppCore;
    use crate::config::{Config, KeyBindAction};
    use crate::frontend::common::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn build_keybind_map_parses_valid_entries() {
        let mut config = Config::default();
        config.keybinds.insert(
            "ctrl+a".to_string(),
            KeyBindAction::Action("copy".to_string()),
        );
        config.keybinds.insert(
            "alt+x".to_string(),
            KeyBindAction::Action("paste".to_string()),
        );

        let map = AppCore::build_keybind_map(&config);
        let ctrl_a = KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::CTRL,
        };
        let alt_x = KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::ALT,
        };

        assert!(map.contains_key(&ctrl_a), "Expected ctrl+a entry");
        assert!(map.contains_key(&alt_x), "Expected alt+x entry");
    }

    #[test]
    fn build_keybind_map_skips_invalid_keys() {
        let mut config = Config::default();
        config.keybinds.insert(
            "ctrl+notakey".to_string(),
            KeyBindAction::Action("copy".to_string()),
        );

        let map = AppCore::build_keybind_map(&config);
        assert!(map.is_empty(), "Invalid keybind should be skipped");
    }
}
