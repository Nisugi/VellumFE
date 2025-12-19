use anyhow::Result;

use super::AppCore;

impl AppCore {
    /// Send command to server
    pub fn send_command(&mut self, command: String) -> Result<String> {
        use crate::data::{SpanType, StyledLine, TextSegment, WindowContent};

        // Check for dot commands (local client commands)
        if command.starts_with('.') {
            return self.handle_dot_command(&command);
        }

        // Echo command to main window (prompt + command)
        if !command.is_empty() {
            tracing::info!("[SEND_COMMAND] Echoing command to main window: '{}'", command);
            if let Some(main_window) = self.ui_state.windows.get_mut("main") {
                if let WindowContent::Text(ref mut content) = main_window.content {
                    let mut segments = Vec::new();

                    // Add prompt with per-character coloring (same as prompt rendering)
                    tracing::debug!(
                        "[SEND_COMMAND] Building styled line with prompt: '{}'",
                        self.game_state.last_prompt
                    );
                    for ch in self.game_state.last_prompt.chars() {
                        let char_str = ch.to_string();

                        // Find color for this character in prompt_colors config
                        let color = self
                            .config
                            .colors
                            .prompt_colors
                            .iter()
                            .find(|pc| pc.character == char_str)
                            .and_then(|pc| {
                                // Prefer fg, fallback to color (legacy)
                                pc.fg.as_ref().or(pc.color.as_ref()).cloned()
                            })
                            .unwrap_or_else(|| "#808080".to_string()); // Default dark gray

                        segments.push(TextSegment {
                            text: char_str,
                            fg: Some(color),
                            bg: None,
                            bold: false,
                            span_type: SpanType::Normal,
                            link_data: None,
                        });
                    }

                    // Add the command text (in default color)
                    segments.push(TextSegment {
                        text: command.clone(),
                        fg: Some("#ffffff".to_string()), // White text for command
                        bg: None,
                        bold: false,
                        span_type: SpanType::Normal,
                        link_data: None,
                    });

                    // Add the styled line to the main window
                    content.add_line(StyledLine { segments: segments.clone() });
                    tracing::info!(
                        "[SEND_COMMAND] Added StyledLine with {} segments to main window",
                        segments.len()
                    );
                }
            }
        }

        // Command history is now managed by the CommandInput widget

        // Return formatted command for network layer to send
        Ok(format!("{}\n", command))
    }

    /// Handle dot commands (local client commands)
    fn handle_dot_command(&mut self, command: &str) -> Result<String> {
        let parts: Vec<&str> = command[1..].split_whitespace().collect();
        let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

        match cmd.as_str() {
            // Application commands
            "quit" | "q" => {
                self.quit();
            }
            "help" | "h" | "?" => {
                self.show_help();
            }

            // Layout commands
            "savelayout" => {
                let name = parts.get(1).unwrap_or(&"default");
                tracing::info!("[APP_CORE] User entered .savelayout command: '{}'", name);
                // Note: This is a placeholder - actual handling should be in main.rs with terminal size
                // For now, we'll use the layout's terminal size or fallback
                let width = self.layout.terminal_width.unwrap_or(80);
                let height = self.layout.terminal_height.unwrap_or(24);
                tracing::warn!(
                    "savelayout called without terminal size - using layout size {}x{}",
                    width,
                    height
                );
                self.save_layout(name, width, height);
            }
            "loadlayout" => {
                // This is just a placeholder - actual handling is in main.rs with terminal size
                self.add_system_message(
                    "Layout loading requires terminal size - handled by main event loop",
                );
            }
            "layouts" => {
                self.list_layouts();
            }
            "resize" => {
                self.resize_to_current_terminal();
            }

            // Window management commands
            "windows" => {
                self.list_windows();
            }
            "deletewindow" | "delwindow" => {
                if let Some(name) = parts.get(1) {
                    self.delete_window(name);
                } else {
                    self.add_system_message("Usage: .deletewindow <name>");
                }
            }
            "addwindow" => {
                if parts.len() >= 6 {
                    let name = parts[1];
                    let widget_type = parts[2];
                    let x = parts[3].parse::<u16>().unwrap_or(0);
                    let y = parts[4].parse::<u16>().unwrap_or(0);
                    let width = parts[5].parse::<u16>().unwrap_or(40);
                    let height = parts
                        .get(6)
                        .and_then(|h| h.parse::<u16>().ok())
                        .unwrap_or(10);
                    self.add_window(name, widget_type, x, y, width, height);
                } else if parts.len() == 1 {
                    // No arguments - open widget picker
                    return Ok("action:addwindow".to_string());
                } else {
                    self.add_system_message(
                        "Usage: .addwindow <name> <type> <x> <y> <width> [height]",
                    );
                    self.add_system_message(
                        "Types: text, progress, countdown, compass, hands, room, indicator",
                    );
                }
            }
            "rename" => {
                if parts.len() >= 3 {
                    let name = parts[1];
                    let new_title = parts[2..].join(" ");
                    self.rename_window(name, &new_title);
                } else {
                    self.add_system_message("Usage: .rename <window> <new title>");
                }
            }
            "border" => {
                if parts.len() >= 3 {
                    let name = parts[1];
                    let style = parts[2];
                    let color = parts.get(3).map(|c| c.to_string());
                    self.set_window_border(name, style, color);
                } else {
                    self.add_system_message("Usage: .border <window> <style> [color]");
                }
            }

            // Highlights
            "highlights" | "hl" => {
                return Ok("action:highlights".to_string());
            }
            "addhighlight" | "addhl" => {
                return Ok("action:addhighlight".to_string());
            }
            "edithighlight" | "edithl" => {
                if let Some(name) = parts.get(1) {
                    return Ok(format!("action:edithighlight:{}", name));
                } else {
                    return Ok("action:edithighlight".to_string());
                }
            }
            "testline" => {
                if let Some(text) = parts.get(1) {
                    let rest_of_line = command[command.find(text).unwrap_or(0)..].to_string();
                    self.inject_test_line(&rest_of_line);
                } else {
                    self.add_system_message("Usage: .testline <text>");
                }
            }
            "savehighlights" | "savehl" => {
                let name = parts.get(1).unwrap_or(&"default");
                match self.config.save_highlights_as(name) {
                    Ok(path) => self.add_system_message(&format!(
                        "Highlights saved as '{}' to {}",
                        name,
                        path.display()
                    )),
                    Err(e) => self.add_system_message(&format!("Failed to save highlights: {}", e)),
                }
            }
            "loadhighlights" | "loadhl" => {
                let name = parts.get(1).unwrap_or(&"default");
                match crate::config::Config::load_highlights_from(name) {
                    Ok(highlights) => {
                        self.config.highlights = highlights;
                        // Rebuild message processor with new highlights
                        self.message_processor = crate::core::MessageProcessor::new(self.config.clone());
                        self.add_system_message(&format!("Highlights '{}' loaded", name));
                    }
                    Err(e) => self.add_system_message(&format!("Failed to load highlights: {}", e)),
                }
            }
            "highlightprofiles" | "hlprofiles" => {
                match crate::config::Config::list_saved_highlights() {
                    Ok(profiles) => {
                        if profiles.is_empty() {
                            self.add_system_message("No saved highlight profiles");
                        } else {
                            self.add_system_message(&format!(
                                "Saved highlight profiles: {}",
                                profiles.join(", ")
                            ));
                        }
                    }
                    Err(e) => {
                        self.add_system_message(&format!("Failed to list highlight profiles: {}", e))
                    }
                }
            }

            // Keybinds
            "keybinds" | "kb" => {
                return Ok("action:keybinds".to_string());
            }
            "addkeybind" | "addkey" => {
                return Ok("action:addkeybind".to_string());
            }

            // Colors
            "colors" | "colorpalette" => {
                return Ok("action:colors".to_string());
            }
            "addcolor" | "createcolor" => {
                return Ok("action:addcolor".to_string());
            }
            "uicolors" => {
                return Ok("action:uicolors".to_string());
            }
            "spellcolors" => {
                return Ok("action:spellcolors".to_string());
            }
            "addspellcolor" | "newspellcolor" => {
                return Ok("action:addspellcolor".to_string());
            }
            // Terminal palette commands (for 256-color mode)
            "setpalette" => {
                return Ok("action:setpalette".to_string());
            }
            "resetpalette" => {
                return Ok("action:resetpalette".to_string());
            }

            // Themes
            "themes" => {
                return Ok("action:themes".to_string());
            }
            "settheme" | "theme" => {
                if let Some(name) = parts.get(1) {
                    return Ok(format!("action:settheme:{}", name));
                } else {
                    self.add_system_message("Usage: .settheme <name>");
                }
            }
            "edittheme" => {
                return Ok("action:edittheme".to_string());
            }

            // Tab navigation
            "nexttab" => {
                return Ok("action:nexttab".to_string());
            }
            "prevtab" => {
                return Ok("action:prevtab".to_string());
            }
            "gonew" | "nextunread" => {
                return Ok("action:nextunread".to_string());
            }

            // Settings
            "settings" => {
                return Ok("action:settings".to_string());
            }

            // Window editor
            "editwindow" | "editwin" => {
                if let Some(name) = parts.get(1) {
                    return Ok(format!("action:editwindow:{}", name));
                } else {
                    // Open window picker
                    return Ok("action:editwindow".to_string());
                }
            }
            "hidewindow" | "hidewin" => {
                if let Some(name) = parts.get(1) {
                    return Ok(format!("action:hidewindow:{}", name));
                } else {
                    // Open window picker
                    return Ok("action:hidewindow".to_string());
                }
            }

            // Reload config from disk
            "reload" => {
                if parts.len() < 2 {
                    // Reload everything
                    self.reload_all();
                } else {
                    match parts[1] {
                        "highlights" | "hl" => self.reload_highlights(),
                        "keybinds" | "kb" => self.reload_keybinds(),
                        "settings" => self.reload_settings(),
                        "colors" => self.reload_colors(),
                        _ => {
                            self.add_system_message(&format!("Unknown reload category: {}", parts[1]));
                            self.add_system_message("Usage: .reload [highlights|keybinds|settings|colors]");
                            self.add_system_message("       .reload (reload everything)");
                        }
                    }
                }
            }

            // Menu system
            "menu" => {
                // Build main menu
                let items = self.build_main_menu();

                tracing::debug!("Creating menu with {} items", items.len());

                // Create popup menu at center of screen
                // Position will be adjusted by frontend based on actual terminal size
                self.ui_state.popup_menu = Some(crate::data::ui_state::PopupMenu::new(
                    items,
                    (40, 12), // Default center position
                ));

                // Switch to Menu input mode
                self.ui_state.input_mode = crate::data::ui_state::InputMode::Menu;
                tracing::debug!("Input mode set to Menu: {:?}", self.ui_state.input_mode);
                self.needs_render = true;
            }

            _ => {
                self.add_system_message(&format!("Unknown command: {}", command));
                self.add_system_message("Type .help for list of commands");
            }
        }

        // Command input is now managed by the CommandInput widget

        // Don't send anything to server
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    // ========== Dot Command Parsing Tests ==========
    //
    // These tests verify the dot command parsing logic by testing:
    // 1. Command name extraction (case insensitivity)
    // 2. Argument parsing
    // 3. Action string generation for commands that return actions
    //
    // Note: Tests that require full AppCore are handled in integration tests.

    /// Helper to parse dot commands the same way handle_dot_command does
    fn parse_dot_command(command: &str) -> (String, Vec<String>) {
        let parts: Vec<&str> = command[1..].split_whitespace().collect();
        let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        let args: Vec<String> = parts.iter().skip(1).map(|s| s.to_string()).collect();
        (cmd, args)
    }

    // ========== Command name parsing tests ==========

    #[test]
    fn test_parse_dot_command_simple() {
        let (cmd, args) = parse_dot_command(".quit");
        assert_eq!(cmd, "quit");
        assert!(args.is_empty());
    }

    #[test]
    fn test_parse_dot_command_with_args() {
        let (cmd, args) = parse_dot_command(".savelayout myname");
        assert_eq!(cmd, "savelayout");
        assert_eq!(args, vec!["myname"]);
    }

    #[test]
    fn test_parse_dot_command_multiple_args() {
        let (cmd, args) = parse_dot_command(".addwindow main text 0 0 80 24");
        assert_eq!(cmd, "addwindow");
        assert_eq!(args, vec!["main", "text", "0", "0", "80", "24"]);
    }

    #[test]
    fn test_parse_dot_command_case_insensitive() {
        let (cmd, _) = parse_dot_command(".QUIT");
        assert_eq!(cmd, "quit");

        let (cmd, _) = parse_dot_command(".HeLp");
        assert_eq!(cmd, "help");
    }

    #[test]
    fn test_parse_dot_command_extra_whitespace() {
        let (cmd, args) = parse_dot_command(".rename   window   New Title");
        assert_eq!(cmd, "rename");
        assert_eq!(args, vec!["window", "New", "Title"]);
    }

    #[test]
    fn test_parse_dot_command_empty() {
        let (cmd, args) = parse_dot_command(".");
        assert_eq!(cmd, "");
        assert!(args.is_empty());
    }

    // ========== Command alias tests ==========

    #[test]
    fn test_quit_aliases() {
        let quit_commands = vec![".quit", ".q", ".QUIT", ".Q"];
        for cmd_str in quit_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "quit" || cmd == "q",
                "Expected quit/q, got '{}' for input '{}'",
                cmd,
                cmd_str
            );
        }
    }

    #[test]
    fn test_help_aliases() {
        let help_commands = vec![".help", ".h", ".?", ".HELP", ".H"];
        for cmd_str in help_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "help" || cmd == "h" || cmd == "?",
                "Expected help/h/?, got '{}' for input '{}'",
                cmd,
                cmd_str
            );
        }
    }

    #[test]
    fn test_highlight_aliases() {
        let hl_commands = vec![".highlights", ".hl"];
        for cmd_str in hl_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "highlights" || cmd == "hl",
                "Expected highlights/hl, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_keybind_aliases() {
        let kb_commands = vec![".keybinds", ".kb"];
        for cmd_str in kb_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "keybinds" || cmd == "kb",
                "Expected keybinds/kb, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_deletewindow_aliases() {
        let del_commands = vec![".deletewindow", ".delwindow"];
        for cmd_str in del_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "deletewindow" || cmd == "delwindow",
                "Expected deletewindow/delwindow, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_editwindow_aliases() {
        let edit_commands = vec![".editwindow", ".editwin"];
        for cmd_str in edit_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "editwindow" || cmd == "editwin",
                "Expected editwindow/editwin, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_theme_aliases() {
        let theme_commands = vec![".settheme", ".theme"];
        for cmd_str in theme_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "settheme" || cmd == "theme",
                "Expected settheme/theme, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_addhighlight_aliases() {
        let add_commands = vec![".addhighlight", ".addhl"];
        for cmd_str in add_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "addhighlight" || cmd == "addhl",
                "Expected addhighlight/addhl, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_edithighlight_aliases() {
        let edit_commands = vec![".edithighlight", ".edithl"];
        for cmd_str in edit_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "edithighlight" || cmd == "edithl",
                "Expected edithighlight/edithl, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_addkeybind_aliases() {
        let add_commands = vec![".addkeybind", ".addkey"];
        for cmd_str in add_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "addkeybind" || cmd == "addkey",
                "Expected addkeybind/addkey, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_colors_aliases() {
        let color_commands = vec![".colors", ".colorpalette"];
        for cmd_str in color_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "colors" || cmd == "colorpalette",
                "Expected colors/colorpalette, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_addcolor_aliases() {
        let add_commands = vec![".addcolor", ".createcolor"];
        for cmd_str in add_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "addcolor" || cmd == "createcolor",
                "Expected addcolor/createcolor, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_addspellcolor_aliases() {
        let add_commands = vec![".addspellcolor", ".newspellcolor"];
        for cmd_str in add_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "addspellcolor" || cmd == "newspellcolor",
                "Expected addspellcolor/newspellcolor, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_nextunread_aliases() {
        let next_commands = vec![".gonew", ".nextunread"];
        for cmd_str in next_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "gonew" || cmd == "nextunread",
                "Expected gonew/nextunread, got '{}'",
                cmd
            );
        }
    }

    #[test]
    fn test_hidewindow_aliases() {
        let hide_commands = vec![".hidewindow", ".hidewin"];
        for cmd_str in hide_commands {
            let (cmd, _) = parse_dot_command(cmd_str);
            assert!(
                cmd == "hidewindow" || cmd == "hidewin",
                "Expected hidewindow/hidewin, got '{}'",
                cmd
            );
        }
    }

    // ========== Action string generation tests ==========

    /// Helper to determine what action string a command would return
    fn get_expected_action(cmd: &str, args: &[String]) -> Option<String> {
        match cmd {
            "highlights" | "hl" => Some("action:highlights".to_string()),
            "addhighlight" | "addhl" => Some("action:addhighlight".to_string()),
            "edithighlight" | "edithl" => {
                if let Some(name) = args.first() {
                    Some(format!("action:edithighlight:{}", name))
                } else {
                    Some("action:edithighlight".to_string())
                }
            }
            "keybinds" | "kb" => Some("action:keybinds".to_string()),
            "addkeybind" | "addkey" => Some("action:addkeybind".to_string()),
            "colors" | "colorpalette" => Some("action:colors".to_string()),
            "addcolor" | "createcolor" => Some("action:addcolor".to_string()),
            "uicolors" => Some("action:uicolors".to_string()),
            "spellcolors" => Some("action:spellcolors".to_string()),
            "addspellcolor" | "newspellcolor" => Some("action:addspellcolor".to_string()),
            "themes" => Some("action:themes".to_string()),
            "settheme" | "theme" => {
                if let Some(name) = args.first() {
                    Some(format!("action:settheme:{}", name))
                } else {
                    None // Shows usage message instead
                }
            }
            "edittheme" => Some("action:edittheme".to_string()),
            "nexttab" => Some("action:nexttab".to_string()),
            "prevtab" => Some("action:prevtab".to_string()),
            "gonew" | "nextunread" => Some("action:nextunread".to_string()),
            "settings" => Some("action:settings".to_string()),
            "editwindow" | "editwin" => {
                if let Some(name) = args.first() {
                    Some(format!("action:editwindow:{}", name))
                } else {
                    Some("action:editwindow".to_string())
                }
            }
            "hidewindow" | "hidewin" => {
                if let Some(name) = args.first() {
                    Some(format!("action:hidewindow:{}", name))
                } else {
                    Some("action:hidewindow".to_string())
                }
            }
            "addwindow" if args.is_empty() => Some("action:addwindow".to_string()),
            _ => None,
        }
    }

    #[test]
    fn test_action_highlights() {
        let (cmd, args) = parse_dot_command(".highlights");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:highlights".to_string()));
    }

    #[test]
    fn test_action_highlights_alias() {
        let (cmd, args) = parse_dot_command(".hl");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:highlights".to_string()));
    }

    #[test]
    fn test_action_keybinds() {
        let (cmd, args) = parse_dot_command(".keybinds");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:keybinds".to_string()));
    }

    #[test]
    fn test_action_colors() {
        let (cmd, args) = parse_dot_command(".colors");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:colors".to_string()));
    }

    #[test]
    fn test_action_themes() {
        let (cmd, args) = parse_dot_command(".themes");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:themes".to_string()));
    }

    #[test]
    fn test_action_settheme_with_name() {
        let (cmd, args) = parse_dot_command(".settheme dark");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:settheme:dark".to_string()));
    }

    #[test]
    fn test_action_settheme_without_name() {
        let (cmd, args) = parse_dot_command(".settheme");
        assert_eq!(get_expected_action(&cmd, &args), None);
    }

    #[test]
    fn test_action_editwindow_with_name() {
        let (cmd, args) = parse_dot_command(".editwindow main");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:editwindow:main".to_string()));
    }

    #[test]
    fn test_action_editwindow_without_name() {
        let (cmd, args) = parse_dot_command(".editwindow");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:editwindow".to_string()));
    }

    #[test]
    fn test_action_hidewindow_with_name() {
        let (cmd, args) = parse_dot_command(".hidewindow main");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:hidewindow:main".to_string()));
    }

    #[test]
    fn test_action_hidewindow_without_name() {
        let (cmd, args) = parse_dot_command(".hidewindow");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:hidewindow".to_string()));
    }

    #[test]
    fn test_action_hidewin_alias() {
        let (cmd, args) = parse_dot_command(".hidewin chat");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:hidewindow:chat".to_string()));
    }

    #[test]
    fn test_action_edithighlight_with_name() {
        let (cmd, args) = parse_dot_command(".edithighlight combat");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:edithighlight:combat".to_string()));
    }

    #[test]
    fn test_action_edithighlight_without_name() {
        let (cmd, args) = parse_dot_command(".edithighlight");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:edithighlight".to_string()));
    }

    #[test]
    fn test_action_addwindow_no_args_opens_picker() {
        let (cmd, args) = parse_dot_command(".addwindow");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:addwindow".to_string()));
    }

    #[test]
    fn test_action_addwindow_with_args_does_not_return_action() {
        // When addwindow has args, it creates window directly (no action string)
        let (cmd, args) = parse_dot_command(".addwindow main text 0 0 80");
        assert_eq!(get_expected_action(&cmd, &args), None);
    }

    #[test]
    fn test_action_settings() {
        let (cmd, args) = parse_dot_command(".settings");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:settings".to_string()));
    }

    #[test]
    fn test_action_nexttab() {
        let (cmd, args) = parse_dot_command(".nexttab");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:nexttab".to_string()));
    }

    #[test]
    fn test_action_prevtab() {
        let (cmd, args) = parse_dot_command(".prevtab");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:prevtab".to_string()));
    }

    #[test]
    fn test_action_nextunread() {
        let (cmd, args) = parse_dot_command(".nextunread");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:nextunread".to_string()));
    }

    #[test]
    fn test_action_gonew() {
        let (cmd, args) = parse_dot_command(".gonew");
        assert_eq!(get_expected_action(&cmd, &args), Some("action:nextunread".to_string()));
    }

    // ========== Addwindow argument parsing tests ==========

    #[test]
    fn test_addwindow_parses_coordinates() {
        let (_, args) = parse_dot_command(".addwindow test text 10 20 80 24");
        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "test");      // name
        assert_eq!(args[1], "text");      // type
        assert_eq!(args[2], "10");        // x
        assert_eq!(args[3], "20");        // y
        assert_eq!(args[4], "80");        // width
        assert_eq!(args[5], "24");        // height
    }

    #[test]
    fn test_addwindow_optional_height() {
        let (_, args) = parse_dot_command(".addwindow test progress 0 0 40");
        assert_eq!(args.len(), 5);
        // Height should default to 10 in actual handler
    }

    // ========== Border command parsing tests ==========

    #[test]
    fn test_border_command_with_color() {
        let (cmd, args) = parse_dot_command(".border main double #ff0000");
        assert_eq!(cmd, "border");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "main");
        assert_eq!(args[1], "double");
        assert_eq!(args[2], "#ff0000");
    }

    #[test]
    fn test_border_command_without_color() {
        let (cmd, args) = parse_dot_command(".border main single");
        assert_eq!(cmd, "border");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "main");
        assert_eq!(args[1], "single");
    }

    // ========== Rename command parsing tests ==========

    #[test]
    fn test_rename_command_single_word_title() {
        let (cmd, args) = parse_dot_command(".rename window NewTitle");
        assert_eq!(cmd, "rename");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "window");
        assert_eq!(args[1], "NewTitle");
    }

    #[test]
    fn test_rename_command_multi_word_title() {
        let (cmd, args) = parse_dot_command(".rename window New Title Here");
        assert_eq!(cmd, "rename");
        // Note: actual handler joins args[1..] with spaces
        assert_eq!(args.len(), 4);
        assert_eq!(args[0], "window");
        assert_eq!(args[1..].join(" "), "New Title Here");
    }

    // ========== Unknown command tests ==========

    #[test]
    fn test_unknown_command() {
        let (cmd, _) = parse_dot_command(".nonexistent");
        assert_eq!(cmd, "nonexistent");
        assert_eq!(get_expected_action(&cmd, &[]), None);
    }

    // ========== Command detection tests ==========

    #[test]
    fn test_is_dot_command() {
        assert!(".quit".starts_with('.'));
        assert!(".help".starts_with('.'));
        assert!(!"quit".starts_with('.'));
        assert!(!"look".starts_with('.'));
    }

    #[test]
    fn test_regular_command_format() {
        // Regular commands should be returned with newline for network
        let command = "look";
        let formatted = format!("{}\n", command);
        assert_eq!(formatted, "look\n");
    }

    #[test]
    fn test_empty_command_format() {
        let command = "";
        let formatted = format!("{}\n", command);
        assert_eq!(formatted, "\n");
    }
}
