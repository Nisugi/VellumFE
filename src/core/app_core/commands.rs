use anyhow::Result;

use super::AppCore;
use crate::data::{CommandOutcome, ShellZoneTarget, UiAction, ZoneOp};

/// Compass/vertical movement words — everything else in a room's wayto
/// edges is a "portal" (go door, climb stair, enter hole, ...).
const COMPASS_WORDS: [&str; 22] = [
    "north", "northeast", "east", "southeast", "south", "southwest", "west", "northwest", "up",
    "down", "out", "n", "ne", "e", "se", "s", "sw", "w", "nw", "u", "d", "o",
];

/// Room-object nouns that look like walkable portals, for the fallback
/// when the mapdb doesn't know the current room.
const PORTAL_NOUNS: [&str; 18] = [
    "door", "gate", "arch", "archway", "portal", "stair", "stairs", "stairway", "steps",
    "ladder", "trapdoor", "opening", "entrance", "path", "trail", "bridge", "ramp", "curtain",
];

/// Distinct non-compass, non-StringProc movement commands from a room's
/// wayto edges, in stable (BTreeMap value) order.
fn portal_candidates<'a>(wayto: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for command in wayto {
        let trimmed = command.trim();
        if crate::core::mapdb::is_proc_command(trimmed) {
            continue;
        }
        if COMPASS_WORDS.contains(&trimmed.to_ascii_lowercase().as_str()) {
            continue;
        }
        if seen.insert(trimmed.to_ascii_lowercase()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

/// Seconds encoded by a macro sleep segment: the whole segment (modulo
/// surrounding spaces) must be `s` followed by a number — `s2`, `s0.5`,
/// `s90` (no upper bound). Anything else is a game command; a bare `s`
/// stays the game's "south".
fn sleep_segment_seconds(segment: &str) -> Option<f64> {
    let digits = segment.trim().strip_prefix('s')?;
    if digits.is_empty()
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        || !digits.chars().any(|c| c.is_ascii_digit())
        || digits.chars().filter(|&c| c == '.').count() > 1
    {
        return None;
    }
    digits.parse().ok()
}

/// Split a multi-command macro containing sleep segments into the part to
/// send immediately (joined by \r, the way multi-command strings always
/// ride to the server) and the paused remainder as (cumulative delay,
/// command) pairs. None when the string has no sleep segments — the
/// normal send path handles it untouched.
fn split_sleep_macro(
    text: &str,
) -> Option<(Option<String>, Vec<(std::time::Duration, String)>)> {
    if !text.contains('\r') || !text.split('\r').any(|s| sleep_segment_seconds(s).is_some()) {
        return None;
    }
    let mut immediate: Vec<&str> = Vec::new();
    let mut delayed: Vec<(std::time::Duration, String)> = Vec::new();
    let mut pause = 0.0_f64;
    for segment in text.split('\r') {
        if let Some(seconds) = sleep_segment_seconds(segment) {
            pause += seconds;
            continue;
        }
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if pause == 0.0 {
            immediate.push(segment);
        } else {
            delayed.push((
                std::time::Duration::from_secs_f64(pause),
                segment.to_string(),
            ));
        }
    }
    Some((
        (!immediate.is_empty()).then(|| immediate.join("\r")),
        delayed,
    ))
}

impl AppCore {
    /// Send command to server
    pub fn send_command(&mut self, command: String) -> Result<CommandOutcome> {
        // Macro sleep segments: `look\rs2.5\rhide` pauses 2.5s between the
        // commands (paused segments go out via take_outbound when due).
        // Only strings containing a sleep segment take this path — plain
        // multi-command macros ride through unchanged.
        if let Some((immediate, delayed)) = split_sleep_macro(&command) {
            for (delay, segment) in delayed {
                self.queue_timed_command(delay, segment);
            }
            return match immediate {
                Some(text) => self.send_command(text),
                None => Ok(CommandOutcome::Handled),
            };
        }

        // Check for dot commands (local client commands). They never reach
        // the server, but they're still user input — echo them like any
        // other command instead of executing silently.
        if command.starts_with('.') {
            self.echo_command_to_main(&command);
            return self.handle_dot_command(&command);
        }

        // If the next room turns out to be unmapped, this command is the
        // edge label on its ghost-room sketch ("go shop").
        self.map.note_command(&command);

        // Intercept game "quit" command - save settings before disconnecting
        // This handles the case where users close terminal after game disconnect
        if command.trim().eq_ignore_ascii_case("quit") {
            self.save_on_quit();
            // Don't set self.running = false - let VellumFE stay open
            // Fall through to send command to server
        }

        // Echo command to windows subscribed to "main" stream
        self.echo_command_to_main(&command);

        // Command history is now managed by the CommandInput widget

        // Return command for network layer to send (network layer adds newline)
        Ok(CommandOutcome::Game(command))
    }

    /// Echo a user-entered command to every window subscribed to "main"
    /// (prompt + command in the configured echo color), honoring the
    /// `command_echo` setting. Shared by the server send path and dot
    /// commands, which execute locally but should still show what was typed.
    pub(crate) fn echo_command_to_main(&mut self, command: &str) {
        use crate::data::{SpanType, StyledLine, TextSegment, WindowContent};

        if !self.config.ui.command_echo || command.is_empty() {
            return;
        }
        // Get windows subscribed to "main" stream
        let subscribers: Vec<String> = self
            .message_processor
            .get_stream_subscribers("main")
            .to_vec();

        tracing::info!(
            "[SEND_COMMAND] Echoing command to {} windows subscribed to 'main': {:?}",
            subscribers.len(),
            subscribers
        );

        // Build the styled line once
        let mut segments = Vec::new();

        // Add prompt with per-character coloring (same as prompt rendering)
        tracing::debug!(
            "[SEND_COMMAND] Building styled line with prompt: '{}'",
            self.game_state.last_prompt
        );
        for ch in self.game_state.last_prompt.chars() {
            // Prebuilt prompt color map (see MessageProcessor::build_prompt_color_map)
            let color = self
                .message_processor
                .prompt_char_color(ch)
                .map(str::to_string)
                .unwrap_or_else(|| "#808080".to_string()); // Default dark gray

            segments.push(TextSegment {
                text: ch.to_string(),
                fg: Some(color),
                bg: None,
                bold: false,
                mono: false,
                span_type: SpanType::Normal,
                link_data: None,
            });
        }

        // Add the command text in the configured echo color
        segments.push(TextSegment {
            text: command.to_string(),
            fg: Some(self.config.colors.ui.command_echo_color.clone()),
            bg: None,
            bold: false,
            mono: false,
            span_type: SpanType::Normal,
            link_data: None,
        });

        let styled_line = StyledLine {
            segments,
            stream: String::from("main"),
            timestamp: None,
        };

        // Echo bypasses the message pipeline, so mirror it to remote
        // clients explicitly (they see the same echo as local windows)
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_text("main", std::sync::Arc::new(styled_line.clone()));
        }

        // Add the styled line to each subscriber window
        for window_name in subscribers {
            if let Some(window) = self.ui_state.windows.get_mut(&window_name) {
                match &mut window.content {
                    WindowContent::Text(ref mut content) => {
                        content.add_line(styled_line.clone());
                        tracing::info!(
                            "[SEND_COMMAND] Added command echo to text window '{}'",
                            window_name
                        );
                    }
                    WindowContent::TabbedText(ref mut tabbed_content) => {
                        // Find tab(s) subscribed to "main" stream and add the line
                        for tab in tabbed_content.tabs.iter_mut() {
                            if tab
                                .definition
                                .streams
                                .iter()
                                .any(|s| s.eq_ignore_ascii_case("main"))
                            {
                                tab.content.add_line(styled_line.clone());
                                tracing::info!(
                                    "[SEND_COMMAND] Added command echo to tabbed window '{}' tab '{}'",
                                    window_name,
                                    tab.definition.name
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// `.webinfo`: the phone-onboarding pairing URL and QR code.
    fn show_webinfo(&mut self) {
        if !self.config.web.enabled {
            self.add_system_message(
                "Web server is disabled. Enable [web] in config.toml or pass --web-port.",
            );
            return;
        }
        let Some(port) = self
            .message_processor
            .remote
            .as_ref()
            .and_then(|remote| remote.bound_port())
        else {
            self.add_system_message("Web server is not running (bind failed or still starting)");
            return;
        };
        let token = match crate::config::Config::load_or_create_web_token() {
            Ok(token) => token,
            Err(e) => {
                self.add_system_message(&format!("Pairing token unavailable: {e:#}"));
                return;
            }
        };
        let host = if self.config.web.bind == "127.0.0.1" {
            "127.0.0.1".to_string()
        } else {
            Self::local_lan_ip().unwrap_or_else(|| self.config.web.bind.clone())
        };
        let url = format!("http://{host}:{port}/#token={token}");
        // Deep link for the iOS/Android apps' Remote login tab: same
        // server, but paired through the app shell (token lands in the
        // phone's Keychain/Keystore instead of a browser).
        let app_url = format!("vellum://remote?host={host}&port={port}&token={token}");
        self.add_system_message(&format!("Web session URL (browser): {url}"));
        self.add_system_message(&format!("VellumFE app link: {app_url}"));
        if self.config.web.bind == "127.0.0.1" {
            self.add_system_message(
                "Note: bind = \"127.0.0.1\" is this PC only. Set [web] bind = \"0.0.0.0\" so phones on your LAN can connect.",
            );
        }
        self.add_system_message(
            "Off-LAN play: use Tailscale/WireGuard. Never expose this port to the open internet.",
        );
        // A QR drawn with unicode blocks depends on font glyph coverage
        // (the GUI font renders them all as tofu boxes), so render a real
        // SVG QR into a local page and pop the default browser instead.
        match Self::write_pairing_page(&url, &app_url) {
            Ok(path) => {
                if crate::platform::open_url(&path.to_string_lossy()).is_ok() {
                    self.add_system_message("Opened the pairing QR in your browser.");
                } else {
                    self.add_system_message(&format!(
                        "Pairing QR written to {} (open it in a browser)",
                        path.display()
                    ));
                }
            }
            Err(e) => {
                tracing::warn!("webinfo pairing page failed: {e:#}");
                self.add_system_message(&format!("Could not write pairing page: {e:#}"));
            }
        }
    }

    /// Write ~/.vellum-fe/pair.html: a dark page with two scannable SVG
    /// QRs — the browser URL and the app deep link — plus the URLs. Lives
    /// next to web-token — same trust domain.
    fn write_pairing_page(url: &str, app_url: &str) -> Result<std::path::PathBuf> {
        use anyhow::Context as _;
        fn qr_svg(data: &str) -> Result<String> {
            use anyhow::Context as _;
            let code = qrcode::QrCode::new(data.as_bytes()).context("QR encode failed")?;
            Ok(code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(320, 320)
                .dark_color(qrcode::render::svg::Color("#000000"))
                .light_color(qrcode::render::svg::Color("#ffffff"))
                .build())
        }
        let browser_svg = qr_svg(url)?;
        let app_svg = qr_svg(app_url)?;
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <title>VellumFE pairing</title>\
             <style>body{{background:#111318;color:#d6d6d6;font-family:ui-monospace,Consolas,monospace;\
             display:flex;flex-wrap:wrap;justify-content:center;align-items:flex-start;gap:32px;padding-top:6vh}}\
             .card{{display:flex;flex-direction:column;align-items:center;gap:18px;max-width:420px}}\
             .qr{{background:#fff;padding:16px;border-radius:12px}}\
             a{{color:#d9b44f;word-break:break-all;max-width:80vw}}\
             .foot{{flex-basis:100%;text-align:center}}</style></head>\
             <body>\
             <div class=\"card\"><h2>VellumFE app</h2><div class=\"qr\">{app_svg}</div>\
             <a href=\"{app_url}\">{app_url}</a>\
             <p>Scan with the phone camera — opens the app's Remote tab.</p></div>\
             <div class=\"card\"><h2>Phone browser</h2><div class=\"qr\">{browser_svg}</div>\
             <a href=\"{url}\">{url}</a></div>\
             <p class=\"foot\">Either pairs the phone with every VellumFE session on this PC.</p>\
             </body></html>"
        );
        let path = crate::config::Config::base_dir()?.join("pair.html");
        std::fs::write(&path, html).context("Failed to write pair.html")?;
        Ok(path)
    }

    /// Best-effort LAN IP: route lookup via an unconnected UDP socket
    /// (no packets are sent).
    fn local_lan_ip() -> Option<String> {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("8.8.8.8:80").ok()?;
        Some(socket.local_addr().ok()?.ip().to_string())
    }

    /// Handle dot commands (local client commands)
    /// `.room`: how the stream's room identifiers resolved against the
    /// mapdb — the ground truth for debugging pathing and the mini map.
    fn show_room_debug(&mut self) {
        use crate::core::map_service::DbState;
        let nav = self.nav_room_id.clone().unwrap_or_else(|| "-".into());
        let lich = self.lich_room_id.clone().unwrap_or_else(|| "-".into());
        self.add_system_message(&format!("Stream ids: nav uid={nav}, lich id={lich}"));
        match self.map.db_state() {
            DbState::Loaded => {}
            state => {
                self.add_system_message(&format!("Mapdb not available ({state:?})"));
                return;
            }
        }
        let Some(room_id) = self.map.current_room_id else {
            self.add_system_message("No mapdb room resolved yet.");
            return;
        };
        let location = self.map.current_location.clone().unwrap_or_else(|| "?".into());
        let mut summary = format!("Resolved: room {room_id} in {location}");
        if let Some(db) = self.map.mapdb() {
            if let Some(room) = db.room(room_id) {
                if let Some(title) = room.title.first() {
                    summary.push_str(&format!(" - {title}"));
                }
                let routable = room
                    .wayto
                    .iter()
                    .filter(|(dest, cmd)| {
                        !crate::core::mapdb::is_proc_command(cmd)
                            && matches!(
                                room.timeto.get(dest),
                                Some(crate::core::mapdb::TimeTo::Seconds(s)) if *s >= 0.0
                            )
                    })
                    .count();
                summary.push_str(&format!(
                    " | {} wayto edges ({} routable)",
                    room.wayto.len(),
                    routable
                ));
                if !room.tags.is_empty() {
                    summary.push_str(&format!(" | tags: {}", room.tags.join(", ")));
                }
            }
        }
        self.add_system_message(&summary);
    }

    /// The current room's portal commands ("go arch", "climb stair"):
    /// the mapdb room's wayto edges, falling back to portal-looking
    /// nouns among the room objects when the map doesn't know the room.
    /// Shared by `.portal` and the dynamic `portals` controller wheel.
    pub fn portal_commands(&self) -> Vec<String> {
        let mut candidates: Vec<String> = Vec::new();
        if let (Some(room_id), Some(db)) = (self.map.current_room_id, self.map.mapdb()) {
            if let Some(room) = db.room(room_id) {
                candidates = portal_candidates(room.wayto.values());
            }
        }
        if candidates.is_empty() {
            candidates = self
                .game_state
                .room_objects
                .iter()
                .filter_map(|obj| {
                    let noun = obj.noun.as_deref()?;
                    PORTAL_NOUNS
                        .contains(&noun.to_ascii_lowercase().as_str())
                        .then(|| format!("go {}", noun))
                })
                .collect();
            candidates.dedup();
        }
        candidates
    }

    /// `.portal [n|text]` — walk through the room's non-compass exit
    /// ("go door", "climb stair", ...). One candidate: walk it. Several:
    /// list them; `.portal 2` or `.portal arch` picks. Candidates come
    /// from `portal_commands`. Returns the movement command to send
    /// upstream, or None.
    fn handle_portal_command(&mut self, args: &[String]) -> Option<String> {
        let mut candidates = self.portal_commands();

        match (candidates.len(), args.first()) {
            (0, _) => {
                self.add_system_message("No portal found here.");
                None
            }
            (1, None) => Some(candidates.remove(0)),
            (_, None) => {
                // Several portals: open a local popup menu — keyboard and
                // controller navigable on the desktop frontends. Phones
                // (whose menus are server-driven) get the listing message.
                let items: Vec<crate::data::ui_state::PopupMenuItem> = candidates
                    .iter()
                    .map(|cmd| crate::data::ui_state::PopupMenuItem {
                        text: cmd.clone(),
                        command: cmd.clone(),
                        disabled: false,
                    })
                    .collect();
                let position = self.last_link_click_pos.unwrap_or((200, 160));
                self.ui_state.popup_menu =
                    Some(crate::data::ui_state::PopupMenu::new(items, position));
                self.ui_state.input_mode = crate::data::ui_state::InputMode::Menu;
                self.needs_render = true;
                let listing = candidates
                    .iter()
                    .enumerate()
                    .map(|(i, cmd)| format!("{}) {}", i + 1, cmd))
                    .collect::<Vec<_>>()
                    .join("  ");
                self.add_system_message(&format!(
                    "Portals: {}  — pick from the menu or .portal <number|word>",
                    listing
                ));
                None
            }
            (_, Some(pick)) => {
                if let Ok(index) = pick.parse::<usize>() {
                    if index >= 1 && index <= candidates.len() {
                        return Some(candidates.remove(index - 1));
                    }
                }
                let needle = pick.to_ascii_lowercase();
                if let Some(found) = candidates
                    .iter()
                    .position(|cmd| cmd.to_ascii_lowercase().contains(&needle))
                {
                    return Some(candidates.remove(found));
                }
                self.add_system_message(&format!("No portal matches '{}'.", pick));
                None
            }
        }
    }

    /// `.uiexport <name> [parts...]` — build a shareable `.vellumpack`
    /// of the UI's config files. The GUI passes its live layout via
    /// `extra_files`; other frontends export the TUI layout only.
    pub fn uiexport_with(&mut self, args: &[String], extra_files: Vec<(String, Vec<u8>)>) {
        let Some(name) = args.first().cloned() else {
            self.add_system_message(&format!(
                "Usage: .uiexport <name> [parts...] — parts: {} (default: all)",
                crate::core::uipack::PARTS.join(", ")
            ));
            return;
        };
        let parts: Vec<String> = if args.len() > 1 {
            args[1..].iter().map(|s| s.to_lowercase()).collect()
        } else {
            crate::core::uipack::PARTS.iter().map(|s| s.to_string()).collect()
        };
        let layout_toml = self.layout.clone().to_share_toml().ok();
        let base = match crate::config::Config::base_dir() {
            Ok(base) => base,
            Err(e) => {
                self.add_system_message(&format!("Export failed: {e:#}"));
                return;
            }
        };
        match crate::core::uipack::export(
            &base,
            &name,
            &parts,
            self.config.character.as_deref(),
            layout_toml,
            self.config.active_skin.as_deref(),
            &extra_files,
        ) {
            Ok((path, included)) => {
                self.add_system_message(&format!(
                    "Exported UI pack '{}' ({}) to {}",
                    name,
                    included.join(", "),
                    path.display()
                ));
                self.add_system_message(
                    "Share the file anywhere — it carries no account or connection settings.",
                );
            }
            Err(e) => self.add_system_message(&format!("Export failed: {e:#}")),
        }
    }

    /// `.uiimport <name|file> [apply]` — preview a pack, or apply it
    /// (with backups) and hot-reload what can be. Returns the pack's
    /// GUI-layout bytes with the pack name so the GUI frontend can
    /// install them as a named checkpoint.
    pub fn uiimport(&mut self, args: &[String]) -> Option<(String, Vec<u8>)> {
        let Some(target) = args.first() else {
            self.add_system_message(
                "Usage: .uiimport <name|file> — preview; add 'apply' to install",
            );
            return None;
        };
        let base = match crate::config::Config::base_dir() {
            Ok(base) => base,
            Err(e) => {
                self.add_system_message(&format!("Import failed: {e:#}"));
                return None;
            }
        };
        let Some(path) = crate::core::uipack::resolve_pack_path(&base, target) else {
            self.add_system_message(&format!(
                "No pack '{}' — pass a name from {}/exports or a file path",
                target,
                base.display()
            ));
            return None;
        };

        if args.get(1).map(String::as_str) != Some("apply") {
            match crate::core::uipack::preview(&path) {
                Ok(preview) => {
                    self.add_system_message(&format!(
                        "Pack {} (VellumFE {}): {}{}",
                        path.display(),
                        preview.manifest.version,
                        preview.manifest.parts.join(", "),
                        preview
                            .manifest
                            .skin
                            .as_deref()
                            .map(|s| format!(" — skin '{s}'"))
                            .unwrap_or_default()
                    ));
                    self.add_system_message(&format!(
                        "{} file(s). Run `.uiimport {} apply` to install — replaced files are backed up.",
                        preview.entries.len(),
                        target
                    ));
                }
                Err(e) => self.add_system_message(&format!("Could not read pack: {e:#}")),
            }
            return None;
        }

        match crate::core::uipack::apply(&base, &path, self.config.character.as_deref()) {
            Ok(outcome) => {
                for note in &outcome.notes {
                    self.add_system_message(&format!("uiimport: {note}"));
                }
                if let Some(dir) = &outcome.backup_dir {
                    self.add_system_message(&format!(
                        "Replaced files backed up to {}",
                        dir.display()
                    ));
                }
                // Hot-reload everything the pack can touch.
                self.reload_keybinds();
                self.reload_highlights();
                self.reload_hotbars();
                self.reload_colors();
                match crate::config::MacrosConfig::load(self.config.character.as_deref()) {
                    Ok(macros) => {
                        self.config.macros = macros;
                        self.config.macros_local = crate::config::MacrosConfig::load_local(
                            self.config.character.as_deref(),
                        )
                        .unwrap_or_default();
                        if let Some(remote) = self.message_processor.remote.as_mut() {
                            remote.set_macros(&self.config.macros);
                        }
                    }
                    Err(e) => {
                        self.add_system_message(&format!("Macros did not reload: {e:#}"))
                    }
                }
                if let Some(skin) = &outcome.skin {
                    self.config.active_skin = Some(skin.clone());
                    let _ = self.save_config();
                    self.add_system_message(&format!(
                        "Active skin set to '{skin}' (the GUI applies it on next load or via Settings > Appearance)"
                    ));
                }
                if let Some(layout) = &outcome.layout_name {
                    self.add_system_message(&format!(
                        "TUI layout installed — load it with .loadlayout {layout}"
                    ));
                }
                let pack_name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "imported".to_string());
                outcome.gui_layout.map(|bytes| (pack_name, bytes))
            }
            Err(e) => {
                self.add_system_message(&format!("Import failed: {e:#}"));
                None
            }
        }
    }

    /// `.tts` — text-to-speech control from any frontend. Subcommands:
    /// `status` (default), `on`, `off`, `mute`, `rate <0.5-3.0>`,
    /// `volume <0.0-1.0>`, `voice <name|default>`, `voices`, `test`, `clear`.
    fn handle_tts_command(&mut self, args: &[String]) {
        match args.first().map(String::as_str).unwrap_or("status") {
            "on" => {
                self.config.tts.enabled = true;
                // Full apply: the manager AND the message processor's config
                // snapshot (the enqueue gate) both need to hear about it.
                self.apply_tts_settings();
                let _ = self.save_config();
                self.add_system_message("TTS enabled.");
            }
            "off" => {
                self.config.tts.enabled = false;
                self.apply_tts_settings();
                let _ = self.save_config();
                self.add_system_message("TTS disabled.");
            }
            "mute" => {
                self.tts_manager.toggle_mute();
                let status = if self.tts_manager.is_muted() {
                    "muted"
                } else {
                    "unmuted"
                };
                self.add_system_message(&format!("TTS {}.", status));
            }
            "rate" => match args.get(1).and_then(|v| v.parse::<f32>().ok()) {
                Some(rate) => {
                    let _ = self.tts_manager.set_rate(rate);
                    self.config.tts.rate = self.tts_manager.rate();
                    let _ = self.save_config();
                    self.add_system_message(&format!("TTS rate: {:.1}", self.tts_manager.rate()));
                }
                None => self.add_system_message("Usage: .tts rate <0.5-3.0>"),
            },
            "volume" => match args.get(1).and_then(|v| v.parse::<f32>().ok()) {
                Some(volume) => {
                    let _ = self.tts_manager.set_volume(volume);
                    self.config.tts.volume = self.tts_manager.volume();
                    let _ = self.save_config();
                    self.add_system_message(&format!(
                        "TTS volume: {:.1}",
                        self.tts_manager.volume()
                    ));
                }
                None => self.add_system_message("Usage: .tts volume <0.0-1.0>"),
            },
            "voice" => {
                let wanted = args[1..].join(" ");
                if wanted.is_empty() {
                    self.add_system_message("Usage: .tts voice <name|default>");
                } else if wanted.eq_ignore_ascii_case("default") {
                    self.config.tts.voice = None;
                    self.tts_manager.set_voice_by_name(None);
                    let _ = self.save_config();
                    self.add_system_message("TTS voice: engine default.");
                } else {
                    self.config.tts.voice = Some(wanted.clone());
                    self.tts_manager.set_voice_by_name(Some(wanted.clone()));
                    let _ = self.save_config();
                    self.add_system_message(&format!("TTS voice: {}", wanted));
                }
            }
            "voices" => {
                let voices = self.tts_manager.available_voices();
                if voices.is_empty() {
                    self.add_system_message(
                        "No voices listed (TTS off, or this platform doesn't enumerate).",
                    );
                } else {
                    self.add_system_message(&format!("TTS voices: {}", voices.join(", ")));
                }
            }
            "test" => {
                if let Err(err) = self
                    .tts_manager
                    .speak_text_now("A giant rat scampers out of the shadows. Roundtime, 5 seconds.")
                {
                    self.add_system_message(&format!("TTS test failed: {}", err));
                } else {
                    self.add_system_message("Speaking test sample.");
                }
            }
            "clear" => {
                self.tts_manager.clear_queue();
                self.add_system_message("TTS queue cleared.");
            }
            "status" => {
                self.add_system_message(&format!(
                    "TTS: {} | {} | rate {:.1} | volume {:.1} | voice {} | {} queued | {}",
                    if self.tts_manager.is_enabled() {
                        "on"
                    } else {
                        "off"
                    },
                    if self.tts_manager.is_muted() {
                        "muted"
                    } else {
                        "unmuted"
                    },
                    self.tts_manager.rate(),
                    self.tts_manager.volume(),
                    self.tts_manager.voice_name().unwrap_or("default"),
                    self.tts_manager.queue_size(),
                    if self.tts_manager.is_speaking() {
                        "speaking"
                    } else {
                        "idle"
                    },
                ));
            }
            other => {
                self.add_system_message(&format!(
                    "Unknown .tts subcommand '{}'. Try: on, off, mute, rate, volume, voice, voices, test, clear, status.",
                    other
                ));
            }
        }
    }

    /// `.mapdb` — map data management from any frontend. Subcommands:
    /// `status` (default), `download`, `remove`, `repo <owner/repo>`.
    /// The `.jinx` asset-manager command. Network operations run off-thread
    /// (`jinx_worker`); `repo` list/add/rm/change edit `repos.toml` inline.
    fn handle_jinx(&mut self, args: &[String]) {
        use crate::core::jinx::worker::Request;

        // Keep the worker's repo-seed gate current with the character's game.
        // (parse_jinx_flags is a free fn below so it can be unit-tested.)
        let game = self.game_type();
        self.jinx_worker.set_game(game);

        // Split flags (--repo=NAME, --force, --dry-run) from positional args.
        let (flags, pos) = match parse_jinx_flags(args) {
            Ok(parsed) => parsed,
            Err(bad) => {
                self.add_system_message(&format!("[jinx] unknown flag '{bad}'"));
                return;
            }
        };
        let JinxFlags { only_repo, force, dry_run } = flags;

        let sub = pos.first().copied().unwrap_or("help");
        match sub {
            "help" | "?" => self.jinx_help(),

            // --- repo management: inline, no network ---
            "repo" => self.handle_jinx_repo(&pos[1..]),

            // --- network commands: off-thread ---
            "list" => {
                let ack = self.jinx_worker.start(Request::List { only_repo });
                self.add_system_message(&ack);
            }
            "search" => match pos.get(1) {
                Some(pattern) => {
                    let ack = self
                        .jinx_worker
                        .start(Request::Search { pattern: pattern.to_string() });
                    self.add_system_message(&ack);
                }
                None => self.add_system_message("[jinx] usage: .jinx search <pattern>"),
            },
            "info" => match pos.get(1) {
                Some(name) => {
                    let ack = self.jinx_worker.start(Request::Info {
                        name: name.to_string(),
                        only_repo,
                    });
                    self.add_system_message(&ack);
                }
                None => self.add_system_message("[jinx] usage: .jinx info <name>"),
            },
            "install" => match pos.get(1) {
                Some(name) => {
                    let ack = self.jinx_worker.start(Request::Install {
                        name: name.to_string(),
                        only_repo,
                        overwrite: force,
                    });
                    self.add_system_message(&ack);
                }
                None => self.add_system_message("[jinx] usage: .jinx install <name> [--repo=<r>]"),
            },
            "update" => match pos.get(1) {
                // Update is install with overwrite; a bare `.jinx update`
                // updates everything (auto-update).
                Some(name) => {
                    let ack = self.jinx_worker.start(Request::Install {
                        name: name.to_string(),
                        only_repo,
                        overwrite: true,
                    });
                    self.add_system_message(&ack);
                }
                None => {
                    let ack = self.jinx_worker.start(Request::AutoUpdate { dry_run });
                    self.add_system_message(&ack);
                }
            },
            "auto-update" => {
                let ack = self.jinx_worker.start(Request::AutoUpdate { dry_run });
                self.add_system_message(&ack);
            }

            other => {
                self.add_system_message(&format!("[jinx] unknown command '{other}'"));
                self.jinx_help();
            }
        }
    }

    /// `.jinx repo ...` — list/add/rm/change repository sources, edited inline
    /// on `repos.toml` (no network). Seeding uses the character's game.
    fn handle_jinx_repo(&mut self, args: &[&str]) {
        let game = self.game_type();
        let mut list = match crate::core::jinx::repo::RepoList::load_or_seed(game) {
            Ok(l) => l,
            Err(e) => {
                self.add_system_message(&format!("[jinx] cannot load repos: {e}"));
                return;
            }
        };
        match args.first().copied().unwrap_or("list") {
            "list" => {
                for repo in &list.repos {
                    self.add_system_message(&format!("  {} — {}", repo.name, repo.url));
                }
                if list.repos.is_empty() {
                    self.add_system_message("[jinx] no repositories configured");
                }
            }
            "add" => match (args.get(1), args.get(2)) {
                (Some(name), Some(url)) => match list.add(name, url) {
                    Ok(()) => match list.save() {
                        Ok(()) => self.add_system_message(&format!("[jinx] added repo '{name}'")),
                        Err(e) => self.add_system_message(&format!("[jinx] save failed: {e}")),
                    },
                    Err(e) => self.add_system_message(&format!("[jinx] {e}")),
                },
                _ => self.add_system_message("[jinx] usage: .jinx repo add <name> <https-url>"),
            },
            "rm" | "remove" => match args.get(1) {
                Some(name) => match list.remove(name) {
                    Ok(()) => match list.save() {
                        Ok(()) => self.add_system_message(&format!("[jinx] removed repo '{name}'")),
                        Err(e) => self.add_system_message(&format!("[jinx] save failed: {e}")),
                    },
                    Err(e) => self.add_system_message(&format!("[jinx] {e}")),
                },
                None => self.add_system_message("[jinx] usage: .jinx repo rm <name>"),
            },
            "change" => match (args.get(1), args.get(2)) {
                (Some(name), Some(url)) => match list.change(name, url) {
                    Ok(()) => match list.save() {
                        Ok(()) => self.add_system_message(&format!("[jinx] repo '{name}' -> {url}")),
                        Err(e) => self.add_system_message(&format!("[jinx] save failed: {e}")),
                    },
                    Err(e) => self.add_system_message(&format!("[jinx] {e}")),
                },
                _ => self.add_system_message("[jinx] usage: .jinx repo change <name> <https-url>"),
            },
            other => self.add_system_message(&format!(
                "[jinx] unknown repo command '{other}' (list|add|rm|change)"
            )),
        }
    }

    fn jinx_help(&mut self) {
        for line in [
            "[jinx] asset manager — download skins, icons, layouts, game data",
            "  .jinx list [--repo=<r>]        list available assets",
            "  .jinx search <pattern>         search asset names",
            "  .jinx info <name>              show details",
            "  .jinx install <name> [--force] install an asset",
            "  .jinx update [<name>]          update one asset, or all if omitted",
            "  .jinx auto-update [--dry-run]  update every installed asset",
            "  .jinx repo list|add|rm|change  manage repositories",
        ] {
            self.add_system_message(line);
        }
    }

    fn handle_mapdb(&mut self, args: &[String]) {
        use crate::core::mapdb_update::UpdateStatus;
        match args.first().map(String::as_str).unwrap_or("status") {
            "status" => {
                let db = match self.map.db_state() {
                    crate::core::map_service::DbState::Loaded => self
                        .map
                        .mapdb()
                        .map(|db| format!("loaded ({} rooms)", db.room_count()))
                        .unwrap_or_else(|| "loaded".to_string()),
                    state => format!("{state:?}"),
                };
                self.add_system_message(&format!("[map] database: {db}"));
                let installed = self
                    .map_updater
                    .installed
                    .clone()
                    .unwrap_or_else(|| "none".to_string());
                self.add_system_message(&format!(
                    "[map] downloaded release: {installed} (repo: {})",
                    self.config.map.mapdb_repo
                ));
                if let UpdateStatus::Downloading {
                    tag,
                    received,
                    total,
                } = &self.map_updater.status
                {
                    let progress = match total {
                        Some(total) => format!(
                            "{:.1} / {:.1} MB",
                            *received as f64 / 1e6,
                            *total as f64 / 1e6
                        ),
                        None => format!("{:.1} MB", *received as f64 / 1e6),
                    };
                    self.add_system_message(&format!("[map] downloading {tag}: {progress}"));
                }
            }
            "download" | "update" => {
                if self.map_updater.in_flight() {
                    self.add_system_message("[map] a download is already running (.mapdb status)");
                    return;
                }
                let repo = self.config.map.mapdb_repo.trim().to_owned();
                if repo.is_empty() {
                    self.add_system_message("[map] no repo configured (.mapdb repo <owner/repo>)");
                    return;
                }
                self.add_system_message(&format!("[map] checking {repo} for the latest release..."));
                self.start_mapdb_download(&repo);
            }
            "remove" => {
                if self.map_updater.in_flight() {
                    self.add_system_message("[map] can't remove while a download is running");
                    return;
                }
                self.remove_downloaded_mapdb();
                self.add_system_message(
                    "[map] downloaded map data removed (Lich folder is the source again, if set)",
                );
            }
            "repo" => {
                let Some(repo) = args.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                    self.add_system_message("usage: .mapdb repo <owner/repo>");
                    return;
                };
                self.config.map.mapdb_repo = repo.to_string();
                match self.save_config() {
                    Ok(()) => self.add_system_message(&format!(
                        "[map] release repo set to {repo} (.mapdb download to fetch)"
                    )),
                    Err(e) => self.add_system_message(&format!("[map] save failed: {e}")),
                }
            }
            other => {
                self.add_system_message(&format!(
                    "[map] unknown subcommand '{other}' - usage: .mapdb [status|download|remove|repo <owner/repo>]"
                ));
            }
        }
    }

    /// `.go2 <target>` — native map travel. Subcommands: `stop`, `status`,
    /// `save <name> [id]`, `targets`, `back`.
    fn handle_go2(&mut self, args: &[String]) {
        use crate::core::travel::target::Resolved;
        let first = args.first().map(String::as_str).unwrap_or("");
        match first {
            "" => {
                self.add_system_message(
                    "usage: .go2 <room id | uid | tag | saved name | text> - also: .go2 stop / status / save <name> [id] / targets / back",
                );
            }
            "stop" => self.stop_travel(),
            "status" => {
                let status = match (self.travel.task(), self.map.mapdb(), self.map.current_room_id)
                {
                    (Some(task), Some(db), Some(current)) => {
                        let done = task.rooms_total() - task.rooms_remaining();
                        format!(
                            "[go2] -> room {}: {}/{} rooms, ETA {}",
                            task.destination,
                            done,
                            task.rooms_total(),
                            crate::core::travel::format_eta(task.eta_seconds(db, current))
                        )
                    }
                    (Some(task), _, _) => format!("[go2] -> room {} (resolving...)", task.destination),
                    _ => "[go2] not traveling.".to_string(),
                };
                self.add_system_message(&status);
            }
            "save" => {
                let Some(name) = args.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                    self.add_system_message("usage: .go2 save <name> [room id] (defaults to the current room)");
                    return;
                };
                if name.parse::<u32>().is_ok() || name.eq_ignore_ascii_case("back") {
                    self.add_system_message("[go2] that name would shadow a room id or keyword - pick another");
                    return;
                }
                let id = match args.get(2) {
                    Some(arg) => match arg.parse::<u32>() {
                        Ok(id) => Some(id),
                        Err(_) => {
                            self.add_system_message(&format!("[go2] '{arg}' is not a room id"));
                            return;
                        }
                    },
                    None => self.map.current_room_id,
                };
                let Some(id) = id else {
                    self.add_system_message(
                        "[go2] current room unknown - give an explicit id: .go2 save <name> <id>",
                    );
                    return;
                };
                self.config.go2.saved.insert(name.to_lowercase(), id);
                match self.save_config() {
                    Ok(()) => self.add_system_message(&format!(
                        "[go2] saved '{}' -> room {id} (travel there with .go2 {})",
                        name.to_lowercase(),
                        name.to_lowercase()
                    )),
                    Err(e) => self.add_system_message(&format!("[go2] save failed: {e}")),
                }
            }
            "targets" => {
                if self.config.go2.saved.is_empty() {
                    self.add_system_message("[go2] no saved targets (.go2 save <name>)");
                } else {
                    let list: Vec<String> = self
                        .config
                        .go2
                        .saved
                        .iter()
                        .map(|(name, id)| format!("{name} -> {id}"))
                        .collect();
                    self.add_system_message(&format!("[go2] saved targets: {}", list.join(", ")));
                }
            }
            _ => {
                let Some(db) = self.map.mapdb().cloned() else {
                    self.add_system_message(
                        "[go2] map database not loaded - configure it in Settings > Map",
                    );
                    return;
                };
                let input = args.join(" ");
                let resolved = crate::core::travel::target::resolve(
                    &db,
                    self.map.current_room_id,
                    &self.config.go2.saved,
                    self.travel.last_start_room,
                    &input,
                );
                match resolved {
                    Resolved::Room(id) => self.start_travel(id),
                    Resolved::Ambiguous(matches) => {
                        self.add_system_message(&format!(
                            "[go2] several rooms match '{input}' - pick one with .go2 <id>:"
                        ));
                        for (id, title) in matches {
                            self.add_system_message(&format!("  {id}  {title}"));
                        }
                    }
                    Resolved::NotFound(reason) => {
                        self.add_system_message(&format!("[go2] {reason}"));
                    }
                }
            }
        }
    }

    /// `.foreach` entry point: parse, gate on the lease, resolve targets
    /// against tracked containers, classify, and start (or dry-run list).
    fn handle_foreach(&mut self, raw: &str) {
        use crate::core::foreach;

        if raw.trim().is_empty() {
            self.add_system_message(
                "[foreach] usage: .foreach [unique] [first N] [after N] [sorted] \
                 [reversed] [attr=]value in <target>[,...]; command; command...",
            );
            self.add_system_message(
                "[foreach] targets: a container name, or inv | worn | feet | floor. \
                 attrs: type (default) | sellable | noun | name | quick; \
                 'item'/'container' substitute in commands; no commands = list \
                 matches. Containers must have been seen open (look in them once).",
            );
            return;
        }

        if self.foreach.is_running() {
            let desc = self
                .foreach
                .task()
                .map(|t| t.desc.clone())
                .unwrap_or_default();
            self.add_system_message(&format!(
                "[foreach] already running ({desc}) - .stop to cancel it first."
            ));
            return;
        }
        if let Some(owner) = self.automation_blocked_by("foreach") {
            self.add_system_message(&format!(
                "[foreach] {} is driving - .stop to cancel it first.",
                owner.desc
            ));
            return;
        }

        let spec = match foreach::parse(raw) {
            Ok(spec) => spec,
            Err(err) => {
                self.add_system_message(&format!("[foreach] {err}"));
                return;
            }
        };

        // Classifier first (needs &mut self), then borrow the cache.
        let data = self.gameobj_data();

        let mut candidates: Vec<foreach::Candidate> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        for (target, optional) in &spec.targets {
            use crate::core::foreach::Target;
            // Gather (id, noun, name, container_id) for the target. For
            // pseudo-targets the item isn't inside a container, so the
            // `container` substitution falls back to the item's own id
            // (harmless — `item` is what these commands use).
            let rows: Vec<(String, String, String, String)> = match target {
                Target::Container(query) => {
                    let Some(container) = self.game_state.objects.find_container(query)
                    else {
                        if *optional {
                            missing.push(query.clone());
                            continue;
                        }
                        self.add_system_message(&format!(
                            "[foreach] no tracked container matches '{query}' - look \
                             in it once so VellumFE sees its contents, or suffix '?' \
                             to skip it."
                        ));
                        let titles = self.game_state.objects.container_titles();
                        if titles.is_empty() {
                            self.add_system_message(
                                "[foreach] (no containers tracked yet - look in one to start)",
                            );
                        } else {
                            self.add_system_message(&format!(
                                "[foreach] tracked: {}",
                                titles.iter().take(12).cloned().collect::<Vec<_>>().join(", ")
                            ));
                        }
                        return;
                    };
                    let ct = container.command_target();
                    container
                        .items
                        .iter()
                        .map(|i| (i.id.clone(), i.noun.clone(), i.name.clone(), ct.clone()))
                        .collect()
                }
                Target::Inv => self
                    .game_state
                    .objects
                    .carried()
                    .iter()
                    .map(|i| (i.id.clone(), i.noun.clone(), i.name.clone(), i.id.clone()))
                    .collect(),
                Target::Worn => self
                    .game_state
                    .objects
                    .worn()
                    .iter()
                    .map(|i| (i.id.clone(), i.noun.clone(), i.name.clone(), i.id.clone()))
                    .collect(),
                Target::AtFeet => self
                    .game_state
                    .objects
                    .at_feet()
                    .iter()
                    .map(|i| (i.id.clone(), i.noun.clone(), i.name.clone(), i.id.clone()))
                    .collect(),
                Target::Floor => self
                    .game_state
                    .objects
                    .ground()
                    .iter()
                    .map(|i| (i.id.clone(), i.noun.clone(), i.name.clone(), i.id.clone()))
                    .collect(),
            };
            for (id, noun, name, container_id) in rows {
                let types = data
                    .types_of(&name, &noun)
                    .iter()
                    .map(|t| t.to_string())
                    .collect();
                let sellable = data
                    .sellable(&name, &noun)
                    .map(|joined| joined.split(',').map(str::to_string).collect())
                    .unwrap_or_default();
                let status = self
                    .game_state
                    .objects
                    .status_of(&id)
                    .copied()
                    .unwrap_or_default();
                candidates.push(foreach::Candidate {
                    id,
                    noun,
                    name,
                    container_id,
                    types,
                    sellable,
                    status,
                });
            }
        }

        // Marked/registered filter needs status only the INVENTORY FULL
        // scan provides. If any candidate that otherwise matches lacks it,
        // trigger a scan and ask the user to re-run (the scan is async; the
        // result lands on the registry a moment later).
        if spec.status.is_active() {
            let need_scan = candidates.iter().any(|c| {
                let type_ok = spec.value_matches_public(c);
                let unknown = (spec.status.marked.is_some() && c.status.marked.is_none())
                    || (spec.status.registered.is_some()
                        && c.status.registered.is_none());
                type_ok && unknown
            });
            if need_scan {
                if let Some(cmd) = self.message_processor.start_inventory_scan() {
                    // Inject the scan command into the outbound path (zero
                    // delay = next drain).
                    self.queue_timed_command(
                        std::time::Duration::from_millis(0),
                        cmd.to_string(),
                    );
                    self.add_system_message(
                        "[foreach] fetching item status (INVENTORY FULL) - re-run \
                         the command in a moment.",
                    );
                } else {
                    self.add_system_message(
                        "[foreach] an item-status scan is already in progress - \
                         re-run shortly.",
                    );
                }
                return;
            }
        }

        let picked: Vec<foreach::Candidate> =
            spec.select(&candidates).into_iter().cloned().collect();
        for query in &missing {
            self.add_system_message(&format!("[foreach] skipping '{query}?' (not tracked)"));
        }
        if picked.is_empty() {
            self.add_system_message("[foreach] no matching items.");
            return;
        }

        if spec.commands.is_empty() {
            // Dry run: list what a command list would act on.
            self.add_system_message(&format!(
                "[foreach] {} matching item{}:",
                picked.len(),
                if picked.len() == 1 { "" } else { "s" }
            ));
            const SHOW: usize = 50;
            for candidate in picked.iter().take(SHOW) {
                let tags = if candidate.types.is_empty() {
                    "-".to_string()
                } else {
                    candidate.types.join(",")
                };
                self.add_system_message(&format!(
                    "  {}  ({})  #{}",
                    candidate.name, tags, candidate.id
                ));
            }
            if picked.len() > SHOW {
                self.add_system_message(&format!("  ... and {} more", picked.len() - SHOW));
            }
            return;
        }

        let mut items = Vec::new();
        for candidate in &picked {
            let steps = {
                let objects = &self.game_state.objects;
                foreach::build_steps(&spec.commands, candidate, |query| {
                    objects.find_container(query).map(|c| c.command_target())
                })
            };
            match steps {
                Ok(steps) => items.push(foreach::WorkItem {
                    id: candidate.id.clone(),
                    name: candidate.name.clone(),
                    steps,
                }),
                Err(err) => {
                    self.add_system_message(&format!("[foreach] {err}"));
                    return;
                }
            }
        }

        let count = items.len();
        let desc = raw.split(';').next().unwrap_or("").trim().to_string();
        self.foreach
            .set_task(foreach::ForeachTask::new(desc, items));
        self.add_system_message(&format!(
            "[foreach] running {} command{} over {} item{} - .stop cancels.",
            spec.commands.len(),
            if spec.commands.len() == 1 { "" } else { "s" },
            count,
            if count == 1 { "" } else { "s" }
        ));
        // Fire the first send now instead of on the next frame.
        self.tick_foreach();
    }

    fn handle_dot_command(&mut self, command: &str) -> Result<CommandOutcome> {
        let parts: Vec<&str> = command[1..].split_whitespace().collect();
        let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        tracing::debug!("handle_dot_command: '{}'", command);

        match cmd.as_str() {
            // Application commands
            "quit" | "q" => {
                self.quit();
            }
            "help" | "h" | "?" => {
                self.show_help();
            }
            "version" | "ver" => {
                self.show_version();
            }

            // Map debug: how the stream's room identifiers resolved against
            // the mapdb (go2 plan phase 2).
            "room" => {
                self.show_room_debug();
            }

            // Native map travel (no Lich needed).
            "go2" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                self.handle_go2(&args);
            }

            // Toggle categorized container-look display (sorter.lic's
            // native cousin). Persisted like any UI setting; `.sorter
            // edit` opens the rules/order/labels editor.
            "sorter" => {
                let sub = parts.get(1).map(|s| s.to_lowercase());
                let target = match sub.as_deref() {
                    Some("on") => true,
                    Some("off") => false,
                    Some("edit") => {
                        return Ok(CommandOutcome::Ui(UiAction::SorterEdit));
                    }
                    None => !self.config.sorter.enabled,
                    Some(other) => {
                        self.add_system_message(&format!(
                            "Usage: .sorter [on|off|edit] (currently {}), got '{}'",
                            if self.config.sorter.enabled { "on" } else { "off" },
                            other
                        ));
                        return Ok(CommandOutcome::Handled);
                    }
                };
                self.config.sorter.enabled = target;
                self.message_processor.set_sorter_enabled(target);
                match self.save_config() {
                    Ok(()) => self.add_system_message(&format!(
                        "Container-look sorting {}.",
                        if target { "on" } else { "off" }
                    )),
                    Err(e) => self.add_system_message(&format!("Sorter toggle saved to session only: {e}")),
                }
            }

            // Batch item commands over tracked containers (foreach.lic's
            // native cousin). Needs raw text - ';' separates commands.
            "foreach" => {
                let raw = command[1..]
                    .splitn(2, char::is_whitespace)
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                self.handle_foreach(&raw);
            }

            // Automation panic button: cancel whatever owns the connection
            // (a go2 trip today; foreach chains later) and everything it
            // drives. Feature-specific cancels (.go2 stop, Esc) still work.
            "stop" => match self.stop_automation() {
                Some(desc) => {
                    self.add_system_message(&format!("Stopped: {}", desc));
                }
                None => {
                    self.add_system_message("Nothing is running.");
                }
            },

            // Map data management from any frontend — on phones this is THE
            // way to get map data (no Settings > Map panel there).
            "mapdb" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                self.handle_mapdb(&args);
            }

            // Asset manager (the native jinx client): download and update
            // skins, icon maps, layouts, and game data from federated repos.
            // Network work runs off-thread (see jinx_worker); repo edits are
            // instant and inline.
            "jinx" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                self.handle_jinx(&args);
            }

            // Data-pack assets (gameobj-data.xml, ...): source tier + age.
            // `.data reload` re-resolves mid-session, e.g. after Lich's
            // `;repo` refreshed its copy. Settings panel surface is owed;
            // these dot-commands are the agreed v1 interface.
            "data" => {
                let sub = parts.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
                match sub.as_str() {
                    "" | "status" => {
                        for line in crate::core::data_pack::status_lines(
                            self.config.map.lich_dir.as_deref(),
                        ) {
                            self.add_system_message(&line);
                        }
                        let data = self.gameobj_data();
                        self.add_system_message(&format!(
                            "gameobj classifier: {} types, {} sellable{}",
                            data.type_count(),
                            data.sellable_count(),
                            if data.skipped.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    ", {} incompatible regex(es) skipped",
                                    data.skipped.len()
                                )
                            }
                        ));
                    }
                    "reload" => {
                        let types = self.reload_data_pack();
                        self.add_system_message(&format!(
                            "Data pack re-resolved: gameobj classifier reloaded \
                             ({types} types)."
                        ));
                    }
                    // `.data update <name>` is a domain-specific alias over the
                    // asset manager: download the named game-data file from a
                    // repo (off-thread), landing it in the local-store tier the
                    // data pack already reads. The post-install effect reloads.
                    "update" => match parts.get(2) {
                        Some(name) => {
                            self.jinx_worker.set_game(self.game_type());
                            let ack = self.jinx_worker.start(
                                crate::core::jinx::worker::Request::Install {
                                    name: name.to_string(),
                                    only_repo: None,
                                    overwrite: true,
                                },
                            );
                            self.add_system_message(&ack);
                        }
                        None => self
                            .add_system_message("Usage: .data update <name> (e.g. gameobj-data.xml)"),
                    },
                    _ => {
                        self.add_system_message("Usage: .data [status|reload|update <name>]");
                    }
                }
            }

            // Web frontend: reload macros.toml (+ the phone-edited local
            // overlay) and push to connected phones
            "reloadmacros" => {
                match crate::config::MacrosConfig::load(self.config.character.as_deref()) {
                    Ok(macros) => {
                        let groups = macros.groups.len();
                        let floating = macros.floating.len();
                        self.config.macros = macros;
                        self.config.macros_local =
                            crate::config::MacrosConfig::load_local(self.config.character.as_deref())
                                .unwrap_or_default();
                        if let Some(remote) = self.message_processor.remote.as_mut() {
                            remote.set_macros(&self.config.macros);
                        }
                        self.add_system_message(&format!(
                            "Reloaded macros.toml: {} group(s), {} floating button(s)",
                            groups, floating
                        ));
                    }
                    Err(e) => {
                        self.add_system_message(&format!("Failed to reload macros.toml: {e:#}"));
                    }
                }
            }

            // Web frontend: show the pairing URL + QR for phone onboarding
            "webinfo" => {
                self.show_webinfo();
            }

            // Shareable UI packs: export the files that make this UI /
            // preview + apply a shared one. The GUI intercepts both to
            // add / install its live layout alongside.
            "uiexport" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                self.uiexport_with(&args, Vec::new());
            }
            "uiimport" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                if self.uiimport(&args).is_some() {
                    self.add_system_message(
                        "This pack also carries a GUI layout — run the import in the GUI to install it.",
                    );
                }
            }

            // Text-to-speech control from any frontend (the GUI also has
            // Settings > Speech; on the TUI and phones this is THE way).
            "tts" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                self.handle_tts_command(&args);
            }

            // Walk the room's non-compass exit (go door / climb stair / ...);
            // built for controller d-pad left, works typed from anywhere.
            "portal" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                if let Some(command) = self.handle_portal_command(&args) {
                    return Ok(CommandOutcome::Game(command));
                }
            }

            // Layout commands. The TUI intercepts both in
            // handle_command_submission with the real terminal size, so these
            // fallbacks only run in frontends without a cell grid (GUI,
            // headless), where TOML cell layouts don't apply.
            "savelayout" => {
                let name = parts.get(1).unwrap_or(&"default");
                tracing::info!("[APP_CORE] User entered .savelayout command: '{}'", name);
                let width = self.layout.terminal_width.unwrap_or(80);
                let height = self.layout.terminal_height.unwrap_or(24);
                self.save_layout(name, width, height);
                self.add_system_message(&format!(
                    "Saved TUI layout '{}' at {}x{} cells (this frontend has no terminal grid; the GUI saves its own window layout automatically).",
                    name, width, height
                ));
            }
            "loadlayout" => {
                self.add_system_message(
                    "TOML layouts are a TUI feature. The GUI manages its own window layout and saves it automatically.",
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
            // Lich WebUI bridge: no args -> handshake + page picker;
            // ".webui <script/page>" -> open that page as a native panel.
            "webui" => match parts.get(1) {
                None => return Ok(CommandOutcome::Ui(UiAction::WebUiPicker)),
                Some(&"off") => return Ok(CommandOutcome::Ui(UiAction::WebUiOff)),
                Some(page) => {
                    return Ok(CommandOutcome::Ui(UiAction::WebUiOpen(page.to_string())))
                }
            },
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
                    return Ok(CommandOutcome::Ui(UiAction::AddWindowPicker));
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
                return Ok(CommandOutcome::Ui(UiAction::Highlights));
            }
            "addhighlight" | "addhl" => {
                return Ok(CommandOutcome::Ui(UiAction::AddHighlight));
            }
            "edithighlight" | "edithl" => {
                return Ok(CommandOutcome::Ui(UiAction::EditHighlight(
                    parts.get(1).map(|name| name.to_string()),
                )));
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
                        crate::config::Config::compile_highlight_patterns(
                            &mut self.config.highlights,
                        );
                        self.message_processor.apply_config(self.config.clone());
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
                    Err(e) => self
                        .add_system_message(&format!("Failed to list highlight profiles: {}", e)),
                }
            }

            // Keybinds
            "keybinds" | "kb" => {
                return Ok(CommandOutcome::Ui(UiAction::Keybinds));
            }
            // Menu keybinds (nav/action keys active while a menu has focus)
            "menukeybinds" | "menukb" => {
                return Ok(CommandOutcome::Ui(UiAction::MenuKeybinds));
            }
            // Controller bindings editor (GUI)
            "controller" => {
                return Ok(CommandOutcome::Ui(UiAction::Controller));
            }
            // Hotbars (hotkey bar definitions)
            "hotbars" | "hotbar" => {
                return Ok(CommandOutcome::Ui(UiAction::Hotbars));
            }
            // Streams (per-stream routing: every known stream and where it goes)
            "streams" => {
                return Ok(CommandOutcome::Ui(UiAction::Streams));
            }
            "addkeybind" | "addkey" => {
                return Ok(CommandOutcome::Ui(UiAction::AddKeybind));
            }
            "savekeybinds" | "savekb" => {
                let name = parts.get(1).unwrap_or(&"default");
                match self.config.save_keybinds_as(name) {
                    Ok(path) => {
                        self.add_system_message(&format!("Keybinds saved to: {}", path.display()));
                    }
                    Err(e) => {
                        self.add_system_message(&format!("Failed to save keybinds: {}", e));
                    }
                }
            }
            "loadkeybinds" | "loadkb" => {
                if let Some(name) = parts.get(1) {
                    match crate::config::Config::load_keybinds_from(name) {
                        Ok(keybinds) => {
                            self.config.keybinds = keybinds;
                            self.rebuild_keybind_map();
                            self.add_system_message(&format!(
                                "Keybinds loaded from profile: {}",
                                name
                            ));
                        }
                        Err(e) => {
                            self.add_system_message(&format!("Failed to load keybinds: {}", e));
                        }
                    }
                } else {
                    self.add_system_message("Usage: .loadkeybinds <profile_name>");
                    self.add_system_message("Use .keybindprofiles to list available profiles");
                }
            }
            "keybindprofiles" | "kbprofiles" => {
                match crate::config::Config::list_saved_keybinds() {
                    Ok(profiles) => {
                        if profiles.is_empty() {
                            self.add_system_message("No saved keybind profiles found.");
                            self.add_system_message("Use .savekeybinds <name> to create one.");
                        } else {
                            self.add_system_message("=== Keybind Profiles ===");
                            for name in profiles {
                                self.add_system_message(&format!(
                                    "  {} - .loadkeybinds {}",
                                    name, name
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        self.add_system_message(&format!("Failed to list keybind profiles: {}", e))
                    }
                }
            }

            // Colors
            "colors" | "colorpalette" => {
                return Ok(CommandOutcome::Ui(UiAction::Colors));
            }
            "addcolor" | "createcolor" => {
                return Ok(CommandOutcome::Ui(UiAction::AddColor));
            }
            "uicolors" => {
                return Ok(CommandOutcome::Ui(UiAction::UiColors));
            }
            "spellcolors" => {
                return Ok(CommandOutcome::Ui(UiAction::SpellColors));
            }
            "addspellcolor" | "newspellcolor" => {
                return Ok(CommandOutcome::Ui(UiAction::AddSpellColor));
            }
            // Terminal palette commands (for 256-color mode)
            "setpalette" => {
                return Ok(CommandOutcome::Ui(UiAction::SetPalette));
            }
            "resetpalette" => {
                return Ok(CommandOutcome::Ui(UiAction::ResetPalette));
            }

            // Themes
            "themes" => {
                return Ok(CommandOutcome::Ui(UiAction::Themes));
            }
            "settheme" | "theme" => {
                if let Some(name) = parts.get(1) {
                    return Ok(CommandOutcome::Ui(UiAction::SetTheme(name.to_string())));
                } else {
                    self.add_system_message("Usage: .settheme <name>");
                }
            }
            "edittheme" => {
                return Ok(CommandOutcome::Ui(UiAction::EditTheme));
            }

            // Skins (GUI graphics layered on top of themes)
            "skins" => {
                return Ok(CommandOutcome::Ui(UiAction::Skins));
            }
            "setskin" | "skin" => {
                if let Some(name) = parts.get(1) {
                    return Ok(CommandOutcome::Ui(UiAction::SetSkin(name.to_string())));
                } else {
                    self.add_system_message("Usage: .setskin <name> (or .setskin none to disable)");
                }
            }
            "makeskin" => {
                if let Some(name) = parts.get(1) {
                    return Ok(CommandOutcome::Ui(UiAction::MakeSkin(name.to_string())));
                } else {
                    self.add_system_message("Usage: .makeskin <name> - create a starter skin");
                }
            }
            "reloadskin" => {
                return Ok(CommandOutcome::Ui(UiAction::ReloadSkin));
            }

            // Tab navigation
            "nexttab" => {
                return Ok(CommandOutcome::Ui(UiAction::NextTab));
            }
            "prevtab" => {
                return Ok(CommandOutcome::Ui(UiAction::PrevTab));
            }
            "gonew" | "nextunread" => {
                return Ok(CommandOutcome::Ui(UiAction::NextUnread));
            }

            // Settings
            "settings" => {
                return Ok(CommandOutcome::Ui(UiAction::Settings));
            }

            // Window editor
            "editwindow" | "editwin" => {
                // No name = open the picker.
                return Ok(CommandOutcome::Ui(UiAction::EditWindow(
                    parts.get(1).map(|name| name.to_string()),
                )));
            }
            "hidewindow" | "hidewin" => {
                // No name = open the picker.
                return Ok(CommandOutcome::Ui(UiAction::HideWindow(
                    parts.get(1).map(|name| name.to_string()),
                )));
            }

            // Shell zones (GUI): show/hide/toggle the header, footer, and
            // sidebars. Plain `.header` toggles, so a single keybind or
            // hotbar button can flip a zone; on/off variants let macros
            // force a known state.
            "header" | "footer" | "leftbar" | "rightbar" => {
                let zone = ShellZoneTarget::parse(&cmd)
                    .expect("arm matches exactly the four zone command names");
                match parts.get(1).map(|s| s.to_ascii_lowercase()).as_deref() {
                    None | Some("toggle") => {
                        return Ok(CommandOutcome::Ui(UiAction::Zone {
                            zone,
                            op: ZoneOp::Toggle,
                        }));
                    }
                    Some("on") | Some("show") => {
                        return Ok(CommandOutcome::Ui(UiAction::Zone { zone, op: ZoneOp::On }));
                    }
                    Some("off") | Some("hide") => {
                        return Ok(CommandOutcome::Ui(UiAction::Zone {
                            zone,
                            op: ZoneOp::Off,
                        }));
                    }
                    Some(other) => {
                        self.add_system_message(&format!(
                            "Usage: .{} [on|off|toggle] (got '{}')",
                            cmd, other
                        ));
                    }
                }
            }

            // Snap diagnostics (GUI): toggle a per-frame trace of the
            // center-window snap engine (gesture classification,
            // canonical vs rendered rects, engaged guides) into
            // vellum-fe.log. Lines are tagged 'snapdbg'.
            "snapdebug" => {
                return Ok(CommandOutcome::Ui(UiAction::SnapDebug));
            }

            // Reload config from disk
            "reload" => {
                tracing::debug!("handle_dot_command: reload args {:?}", parts.get(1));
                if parts.len() < 2 {
                    // Reload everything
                    self.reload_all();
                } else {
                    match parts[1] {
                        "highlights" | "hl" => self.reload_highlights(),
                        "keybinds" | "kb" => self.reload_keybinds(),
                        "hotbars" => self.reload_hotbars(),
                        "settings" => self.reload_settings(),
                        "colors" => self.reload_colors(),
                        "layout" => self.reload_layout(),
                        _ => {
                            self.add_system_message(&format!(
                                "Unknown reload category: {}",
                                parts[1]
                            ));
                            self.add_system_message(
                                "Usage: .reload [highlights|keybinds|hotbars|settings|colors|layout]",
                            );
                            self.add_system_message("       .reload (reload everything)");
                        }
                    }
                }
                tracing::debug!("handle_dot_command: reload complete");
            }

            "transparent" => {
                self.toggle_transparent_background_all();
            }

            // Lock/unlock every window at once. THE lock flag is the shared
            // layout's `WindowBase::locked` — the same one `.lockwindow`,
            // the GUI context menu, and the TUI window editor write — so
            // global and per-window locks compose and both frontends
            // enforce the result.
            "lockwindows" | "lockall" | "unlockwindows" | "unlockall" => {
                let forced = if cmd.starts_with("unlock") {
                    Some(false)
                } else {
                    match parts.get(1).map(|s| s.to_ascii_lowercase()).as_deref() {
                        Some("on") | Some("lock") => Some(true),
                        Some("off") | Some("unlock") => Some(false),
                        _ => None, // bare = toggle
                    }
                };
                let new_state = forced
                    .unwrap_or_else(|| !self.layout.windows.iter().any(|w| w.base().locked));
                for window in &mut self.layout.windows {
                    window.base_mut().locked = new_state;
                }
                if new_state {
                    self.add_system_message("All windows locked (cannot be moved/resized)");
                } else {
                    self.add_system_message("All windows unlocked (can be moved/resized)");
                }
                self.schedule_layout_autosave();
                self.needs_render = true;
            }

            // One window by layout name: `.lockwindow main [on|off]` (bare
            // toggles); `.unlockwindow main` forces off.
            "lockwindow" | "unlockwindow" => match parts.get(1) {
                None => {
                    self.add_system_message(
                        "Usage: .lockwindow <window> [on|off] — .unlockwindow <window> forces off",
                    );
                }
                Some(name) => {
                    let forced = if cmd == "unlockwindow" {
                        Some(false)
                    } else {
                        match parts.get(2).map(|s| s.to_ascii_lowercase()).as_deref() {
                            Some("on") | Some("lock") => Some(true),
                            Some("off") | Some("unlock") => Some(false),
                            _ => None,
                        }
                    };
                    let target = name.to_ascii_lowercase();
                    match self
                        .layout
                        .windows
                        .iter_mut()
                        .find(|w| w.base().name.to_ascii_lowercase() == target)
                    {
                        Some(window) => {
                            let new_state = forced.unwrap_or(!window.base().locked);
                            window.base_mut().locked = new_state;
                            let display = window.base().name.clone();
                            self.add_system_message(&format!(
                                "Window '{}' {}",
                                display,
                                if new_state {
                                    "locked (cannot be moved/resized)"
                                } else {
                                    "unlocked (can be moved/resized)"
                                },
                            ));
                            self.schedule_layout_autosave();
                            self.needs_render = true;
                        }
                        None => {
                            self.add_system_message(&format!("No window named '{}'", name));
                        }
                    }
                }
            },

            "hidecontainers" => {
                // No args = close all, with arg = close matching container
                let args = parts.get(1..).unwrap_or(&[]).join(" ");
                if args.is_empty() {
                    self.close_all_ephemeral_windows();
                } else {
                    self.close_ephemeral_window_by_title(&args);
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
        Ok(CommandOutcome::Handled)
    }
}

/// Parsed `.jinx` flags, split from the positional args.
#[derive(Debug)]
struct JinxFlags {
    only_repo: Option<String>,
    force: bool,
    dry_run: bool,
}

/// Split `.jinx` args into flags and positionals. Returns `Err(flag)` for an
/// unrecognized `--flag`. A free function so it's unit-testable without an
/// `AppCore`.
fn parse_jinx_flags(args: &[String]) -> Result<(JinxFlags, Vec<&str>), String> {
    let mut flags = JinxFlags {
        only_repo: None,
        force: false,
        dry_run: false,
    };
    let mut pos: Vec<&str> = Vec::new();
    for arg in args {
        if let Some(rest) = arg.strip_prefix("--repo=") {
            flags.only_repo = Some(rest.to_string());
        } else if arg == "--force" {
            flags.force = true;
        } else if arg == "--dry-run" {
            flags.dry_run = true;
        } else if arg.starts_with("--") {
            return Err(arg.clone());
        } else {
            pos.push(arg);
        }
    }
    Ok((flags, pos))
}

#[cfg(test)]
mod jinx_command_tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn flags_split_from_positionals() {
        let a = args("install parchment --repo=skins --force");
        let (flags, pos) = parse_jinx_flags(&a).unwrap();
        assert_eq!(pos, ["install", "parchment"]);
        assert_eq!(flags.only_repo.as_deref(), Some("skins"));
        assert!(flags.force);
        assert!(!flags.dry_run);
    }

    #[test]
    fn dry_run_and_bare_positionals() {
        let a = args("auto-update --dry-run");
        let (flags, pos) = parse_jinx_flags(&a).unwrap();
        assert_eq!(pos, ["auto-update"]);
        assert!(flags.dry_run);
        assert!(flags.only_repo.is_none());

        let b = args("list");
        let (_, pos) = parse_jinx_flags(&b).unwrap();
        assert_eq!(pos, ["list"]);
    }

    #[test]
    fn unknown_flag_is_rejected() {
        let a = args("install x --bogus");
        let err = parse_jinx_flags(&a).unwrap_err();
        assert_eq!(err, "--bogus");
    }
}

#[cfg(test)]
mod command_echo_tests {
    use super::*;
    use crate::config::PromptColor;
    use crate::data::{WindowContent, WindowState};

    #[test]
    fn sent_command_echo_uses_configured_color_and_respects_prompt_styles_and_toggle() {
        let mut core = AppCore::new_for_test();
        core.config.colors.ui.command_echo_color = "#123456".to_string();
        core.config.colors.prompt_colors = vec![
            PromptColor {
                character: "R".to_string(),
                fg: Some("#aa0000".to_string()),
                bg: None,
                color: None,
            },
            PromptColor {
                character: ">".to_string(),
                fg: Some("#00aa00".to_string()),
                bg: None,
                color: None,
            },
        ];
        core.message_processor.apply_config(core.config.clone());
        core.game_state.last_prompt = "R>".to_string();

        let mut main_window = WindowState::new_text("main", 100);
        if let WindowContent::Text(content) = &mut main_window.content {
            content.streams = vec!["main".to_string()];
        }
        core.ui_state.windows.insert("main".to_string(), main_window);
        core.message_processor
            .update_text_stream_subscribers(&core.ui_state);

        core.send_command("look".to_string()).unwrap();

        let WindowContent::Text(content) = &core.ui_state.windows["main"].content else {
            panic!("main should be a text window");
        };
        assert_eq!(content.lines.len(), 1);
        let segments = &content.lines[0].segments;
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "R");
        assert_eq!(segments[0].fg.as_deref(), Some("#aa0000"));
        assert_eq!(segments[1].text, ">");
        assert_eq!(segments[1].fg.as_deref(), Some("#00aa00"));
        assert_eq!(segments[2].text, "look");
        assert_eq!(segments[2].fg.as_deref(), Some("#123456"));

        core.config.ui.command_echo = false;
        core.send_command("glance".to_string()).unwrap();

        let WindowContent::Text(content) = &core.ui_state.windows["main"].content else {
            panic!("main should be a text window");
        };
        assert_eq!(content.lines.len(), 1);
    }
}

#[cfg(test)]
mod portal_tests {
    use super::*;
    use crate::core::state::RoomObject;

    #[test]
    fn candidates_skip_compass_and_procs_and_dupes() {
        let wayto: Vec<String> = [
            "north",
            "sw",
            "go door",
            "go door", // second edge through the same door
            "climb stair",
            ";e StringProc stuff",
            "Out",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            portal_candidates(wayto.iter()),
            vec!["go door".to_string(), "climb stair".to_string()]
        );
    }

    #[test]
    fn fallback_uses_portal_nouns_from_room_objects() {
        let mut core = AppCore::new_for_test();
        core.game_state.room_objects = vec![
            RoomObject {
                name: "a wooden door".into(),
                noun: Some("door".into()),
                id: "1".into(),
            },
            RoomObject {
                name: "a silver ring".into(),
                noun: Some("ring".into()),
                id: "2".into(),
            },
        ];
        assert_eq!(core.handle_portal_command(&[]), Some("go door".into()));
    }

    #[test]
    fn no_candidates_reports_and_returns_none() {
        let mut core = AppCore::new_for_test();
        assert_eq!(core.handle_portal_command(&[]), None);
    }

    #[test]
    fn portals_wheel_builds_from_the_room_and_shadows_config() {
        let mut core = AppCore::new_for_test();
        // Empty room: no wheel, same as an empty static one.
        assert_eq!(core.wheel_slices("portals", &[]), None);
        assert_eq!(core.wheel_pick_command("portals", &[0]), None);

        core.game_state.room_objects = vec![
            RoomObject {
                name: "a wooden door".into(),
                noun: Some("door".into()),
                id: "1".into(),
            },
            RoomObject {
                name: "a stone arch".into(),
                noun: Some("arch".into()),
                id: "2".into(),
            },
        ];
        // A static wheel named "portals" is shadowed by the dynamic one.
        core.config.controller_wheels.insert(
            "portals".into(),
            vec![crate::config::WheelSlice {
                label: "static".into(),
                command: "static".into(),
                ..Default::default()
            }],
        );

        let slices = core.wheel_slices("portals", &[]).unwrap();
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].label, "door"); // "go door" minus the verb
        assert_eq!(slices[0].command, "go door");
        assert!(!slices[0].is_folder());

        assert_eq!(core.wheel_pick_command("portals", &[1]), Some("go arch".into()));
        // Flat wheel: folder paths and out-of-range indexes resolve to
        // nothing.
        assert_eq!(core.wheel_slices("portals", &[0]), None);
        assert_eq!(core.wheel_pick_command("portals", &[0, 1]), None);
        assert_eq!(core.wheel_pick_command("portals", &[7]), None);

        // Other keys still hit static config.
        core.config.controller_wheels.insert(
            "spells".into(),
            vec![crate::config::WheelSlice {
                label: "prep".into(),
                command: "prep 101".into(),
                ..Default::default()
            }],
        );
        assert_eq!(core.wheel_pick_command("spells", &[0]), Some("prep 101".into()));
    }

    #[test]
    fn multiple_candidates_need_a_pick() {
        let mut core = AppCore::new_for_test();
        core.game_state.room_objects = vec![
            RoomObject {
                name: "a wooden door".into(),
                noun: Some("door".into()),
                id: "1".into(),
            },
            RoomObject {
                name: "a stone arch".into(),
                noun: Some("arch".into()),
                id: "2".into(),
            },
        ];
        // Ambiguous with no pick: opens the local picker menu, sends nothing.
        assert_eq!(core.handle_portal_command(&[]), None);
        let menu = core.ui_state.popup_menu.as_ref().expect("portal picker menu");
        assert_eq!(menu.get_items().len(), 2);
        assert_eq!(menu.get_items()[0].command, "go door");
        assert_eq!(
            core.ui_state.input_mode,
            crate::data::ui_state::InputMode::Menu
        );
        core.ui_state.popup_menu = None;
        core.ui_state.input_mode = crate::data::ui_state::InputMode::Normal;
        // Pick by number and by word.
        assert_eq!(
            core.handle_portal_command(&["2".into()]),
            Some("go arch".into())
        );
        assert_eq!(
            core.handle_portal_command(&["door".into()]),
            Some("go door".into())
        );
        // Bad picks send nothing.
        assert_eq!(core.handle_portal_command(&["9".into()]), None);
        assert_eq!(core.handle_portal_command(&["window".into()]), None);
    }
}

#[cfg(test)]
mod tests {
    use super::{sleep_segment_seconds, split_sleep_macro};
    use std::time::Duration;

    #[test]
    fn sleep_segments_parse_seconds() {
        assert_eq!(sleep_segment_seconds("s2"), Some(2.0));
        assert_eq!(sleep_segment_seconds("s0.1"), Some(0.1));
        assert_eq!(sleep_segment_seconds(" s3.2 "), Some(3.2)); // spaces ok
        assert_eq!(sleep_segment_seconds("s90"), Some(90.0)); // no max
        // Game commands stay game commands.
        assert_eq!(sleep_segment_seconds("s"), None); // south
        assert_eq!(sleep_segment_seconds("sw"), None);
        assert_eq!(sleep_segment_seconds("stance defensive"), None);
        assert_eq!(sleep_segment_seconds("s1.2.3"), None);
        assert_eq!(sleep_segment_seconds("s."), None);
        assert_eq!(sleep_segment_seconds("s1e3"), None);
        assert_eq!(sleep_segment_seconds("look"), None);
    }

    #[test]
    fn sleep_macros_split_into_immediate_and_delayed() {
        // No sleep segments: the normal path handles it untouched.
        assert_eq!(split_sleep_macro("look"), None);
        assert_eq!(split_sleep_macro("hide\rlook"), None);

        // command\rs3.2\rcommand — and the spaced variant.
        for text in ["hide\rs3.2\rlook", "hide\r s3.2 \r look"] {
            let (immediate, delayed) = split_sleep_macro(text).unwrap();
            assert_eq!(immediate.as_deref(), Some("hide"));
            assert_eq!(
                delayed,
                vec![(Duration::from_secs_f64(3.2), "look".to_string())]
            );
        }

        // Leading sleep: nothing immediate. Consecutive sleeps accumulate.
        let (immediate, delayed) = split_sleep_macro("s1\rlook\rs2\rs0.5\rhide").unwrap();
        assert_eq!(immediate, None);
        assert_eq!(
            delayed,
            vec![
                (Duration::from_secs(1), "look".to_string()),
                (Duration::from_secs_f64(3.5), "hide".to_string()),
            ]
        );

        // Segments before the first sleep ride together, as today.
        let (immediate, delayed) = split_sleep_macro("n\rn\rs2\rlook").unwrap();
        assert_eq!(immediate.as_deref(), Some("n\rn"));
        assert_eq!(delayed.len(), 1);

        // Trailing \r (wrayth-style macros) doesn't produce a phantom
        // command.
        let (immediate, delayed) = split_sleep_macro("hide\rs1\rlook\r").unwrap();
        assert_eq!(immediate.as_deref(), Some("hide"));
        assert_eq!(delayed, vec![(Duration::from_secs(1), "look".to_string())]);
    }

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

    // ========== Typed UI-action outcome tests ==========
    //
    // These exercise the REAL dispatcher (send_command) instead of a
    // hand-maintained mirror of it: the old `get_expected_action` helper
    // was a third copy of the emit mapping and could drift exactly like
    // the frontend string-matches did.

    use crate::core::AppCore;
    use crate::data::{CommandOutcome, UiAction};

    fn ui_outcome(command: &str) -> CommandOutcome {
        let mut core = AppCore::new_for_test();
        core.send_command(command.to_string())
            .expect("command should not error")
    }

    #[test]
    fn editor_commands_return_their_ui_actions() {
        let cases: Vec<(&str, UiAction)> = vec![
            (".highlights", UiAction::Highlights),
            (".hl", UiAction::Highlights),
            (".addhighlight", UiAction::AddHighlight),
            (
                ".edithighlight bandits",
                UiAction::EditHighlight(Some("bandits".into())),
            ),
            (".edithighlight", UiAction::EditHighlight(None)),
            (".keybinds", UiAction::Keybinds),
            (".kb", UiAction::Keybinds),
            (".hotbars", UiAction::Hotbars),
            (".streams", UiAction::Streams),
            (".addkeybind", UiAction::AddKeybind),
            (".colors", UiAction::Colors),
            (".addcolor", UiAction::AddColor),
            (".uicolors", UiAction::UiColors),
            (".spellcolors", UiAction::SpellColors),
            (".addspellcolor", UiAction::AddSpellColor),
            (".themes", UiAction::Themes),
            (".settheme gruvbox", UiAction::SetTheme("gruvbox".into())),
            (".theme gruvbox", UiAction::SetTheme("gruvbox".into())),
            (".edittheme", UiAction::EditTheme),
            (".skins", UiAction::Skins),
            (".setskin wrayth", UiAction::SetSkin("wrayth".into())),
            (".makeskin mine", UiAction::MakeSkin("mine".into())),
            (".reloadskin", UiAction::ReloadSkin),
            (".nexttab", UiAction::NextTab),
            (".prevtab", UiAction::PrevTab),
            (".gonew", UiAction::NextUnread),
            (".nextunread", UiAction::NextUnread),
            (".settings", UiAction::Settings),
            (".editwindow main", UiAction::EditWindow(Some("main".into()))),
            (".editwindow", UiAction::EditWindow(None)),
            (".hidewindow main", UiAction::HideWindow(Some("main".into()))),
            (".hidewindow", UiAction::HideWindow(None)),
            (".hidewin main", UiAction::HideWindow(Some("main".into()))),
            (".addwindow", UiAction::AddWindowPicker),
            (".menukeybinds", UiAction::MenuKeybinds),
            (".controller", UiAction::Controller),
            (".snapdebug", UiAction::SnapDebug),
            (".webui", UiAction::WebUiPicker),
            (".webui off", UiAction::WebUiOff),
            (".webui bigshot", UiAction::WebUiOpen("bigshot".into())),
            (".sorter edit", UiAction::SorterEdit),
        ];
        for (command, expected) in cases {
            assert_eq!(
                ui_outcome(command),
                CommandOutcome::Ui(expected),
                "wrong outcome for {command}"
            );
        }
    }

    #[test]
    fn zone_commands_return_zone_actions() {
        use crate::data::{ShellZoneTarget, ZoneOp};
        assert_eq!(
            ui_outcome(".header"),
            CommandOutcome::Ui(UiAction::Zone {
                zone: ShellZoneTarget::Header,
                op: ZoneOp::Toggle
            })
        );
        assert_eq!(
            ui_outcome(".leftbar on"),
            CommandOutcome::Ui(UiAction::Zone {
                zone: ShellZoneTarget::LeftBar,
                op: ZoneOp::On
            })
        );
        assert_eq!(
            ui_outcome(".footer hide"),
            CommandOutcome::Ui(UiAction::Zone {
                zone: ShellZoneTarget::Footer,
                op: ZoneOp::Off
            })
        );
    }

    #[test]
    fn usage_paths_are_handled_not_actions() {
        // Missing required args show usage instead of emitting an action.
        assert_eq!(ui_outcome(".settheme"), CommandOutcome::Handled);
        assert_eq!(ui_outcome(".setskin"), CommandOutcome::Handled);
        assert_eq!(ui_outcome(".makeskin"), CommandOutcome::Handled);
        // Unknown commands are handled (with a help hint), never sent.
        assert_eq!(ui_outcome(".nonexistent"), CommandOutcome::Handled);
    }

    #[test]
    fn game_commands_pass_through_as_game_outcomes() {
        assert_eq!(ui_outcome("look"), CommandOutcome::Game("look".to_string()));
    }

    // ========== Addwindow argument parsing tests ==========

    #[test]
    fn test_addwindow_parses_coordinates() {
        let (_, args) = parse_dot_command(".addwindow test text 10 20 80 24");
        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "test"); // name
        assert_eq!(args[1], "text"); // type
        assert_eq!(args[2], "10"); // x
        assert_eq!(args[3], "20"); // y
        assert_eq!(args[4], "80"); // width
        assert_eq!(args[5], "24"); // height
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
        // The Handled outcome for unknown commands is asserted in
        // usage_paths_are_handled_not_actions.
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

#[cfg(test)]
mod foreach_tests {
    use crate::core::AppCore;

    fn core_with_bandolier() -> AppCore {
        use crate::core::game_objects::GameItem;
        let mut core = AppCore::new_for_test();
        let objects = &mut core.game_state.objects;
        objects.register_container("77".to_string(), "Bandolier".to_string(), Some("#77".to_string()));
        objects.add_container_item("77", GameItem::new("101", "crystal", "quartz crystal"));
        objects.add_container_item("77", GameItem::new("102", "sword", "slim short sword"));
        core
    }

    #[test]
    fn menu_commands_do_not_recurse_infinitely() {
        // Repro for the menu-Enter stack overflow: drive send_command with
        // exactly what a menu-Enter dispatches. If any recurses, this test
        // stack-overflows and names the frame.
        let mut core = AppCore::new_for_test();
        for cmd in [
            ".menu",
            ".windows",
            "", // empty-menu placeholder command
            "__SUBMENU__windows",
            "__TOGGLE_WINDOW__stow",
            "menu:windows",
            "menu:knownwindows",
        ] {
            let _ = core.send_command(cmd.to_string());
        }
    }

    #[test]
    fn foreach_end_to_end_over_tracked_container() {
        let mut core = core_with_bandolier();
        let _ = core.handle_dot_command(".foreach gem in bandolier; sell item");

        // Runs under the lease as root owner...
        assert!(core.foreach.is_running());
        assert_eq!(core.automation_owner().unwrap().kind, "foreach");
        // ...matched only the gem (bundled classifier), and the start
        // tick fired the implicit get with the exist id.
        assert_eq!(core.take_outbound(), vec!["get #101".to_string()]);

        // .stop cancels the run through the lease.
        let _ = core.handle_dot_command(".stop");
        assert!(!core.foreach.is_running());
        assert!(core.automation_owner().is_none());
    }

    #[test]
    fn foreach_stow_uses_object_target_not_stream_id() {
        // Regression: stow's stream id is "stow" but game commands need
        // the shroud's object id (from the <container> target attribute).
        use crate::core::game_objects::GameItem;
        let mut core = AppCore::new_for_test();
        {
            let objects = &mut core.game_state.objects;
            objects.register_container(
                "stow".to_string(),
                "My Shroud".to_string(),
                Some("#225766691".to_string()),
            );
            objects
                .add_container_item("stow", GameItem::new("333", "crystal", "quartz crystal"));
        }
        // 'container' must substitute to the object id, never "#stow".
        let _ = core.handle_dot_command(".foreach gem in shroud; put item in container");
        assert!(core.foreach.is_running());
        assert_eq!(
            core.take_outbound(),
            vec!["put #333 in #225766691".to_string()]
        );
    }

    #[test]
    fn foreach_worn_and_floor_pseudo_targets() {
        use crate::core::game_objects::GameItem;
        let mut core = AppCore::new_for_test();
        {
            let o = &mut core.game_state.objects;
            // A gem worn (odd, but exercises worn), a non-gem worn.
            o.set_worn(vec![
                GameItem::new("10", "sapphire", "blue sapphire"),
                GameItem::new("11", "cloak", "wool cloak"),
            ]);
            // A gem on the ground.
            o.set_ground(vec![GameItem::new("20", "crystal", "quartz crystal")]);
        }

        // worn target: only the gem matches; item substitution uses its id.
        let _ = core.handle_dot_command(".foreach gem in worn; get item");
        assert!(core.foreach.is_running());
        assert_eq!(core.take_outbound(), vec!["get #10".to_string()]);
        let _ = core.handle_dot_command(".stop");

        // floor target reads registry ground.
        let _ = core.handle_dot_command(".foreach gem in floor; get item");
        assert!(core.foreach.is_running());
        assert_eq!(core.take_outbound(), vec!["get #20".to_string()]);
        let _ = core.handle_dot_command(".stop");
    }

    #[test]
    fn foreach_marked_filter_triggers_scan_then_filters() {
        use crate::core::game_objects::{GameItem, ItemStatus};
        let mut core = AppCore::new_for_test();
        {
            let o = &mut core.game_state.objects;
            o.register_container("77".to_string(), "Bandolier".to_string(), Some("#77".to_string()));
            o.add_container_item("77", GameItem::new("101", "crystal", "quartz crystal"));
            o.add_container_item("77", GameItem::new("102", "crystal", "smoky quartz crystal"));
        }

        // Status unknown → the filter triggers an INVENTORY FULL scan and
        // defers, rather than running on incomplete data.
        let _ = core.handle_dot_command(".foreach marked gem in bandolier; get item");
        assert!(!core.foreach.is_running(), "deferred pending scan");
        assert_eq!(core.take_outbound(), vec!["inventory full".to_string()]);

        // Simulate the scan result landing on the registry.
        core.game_state.objects.set_status(
            "101".to_string(),
            ItemStatus { marked: Some(true), registered: Some(false) },
        );
        core.game_state.objects.set_status(
            "102".to_string(),
            ItemStatus { marked: Some(false), registered: Some(false) },
        );

        // Re-run: now status is known, only the marked gem runs.
        let _ = core.handle_dot_command(".foreach marked gem in bandolier; get item");
        assert!(core.foreach.is_running());
        assert_eq!(core.take_outbound(), vec!["get #101".to_string()]);
    }

    #[test]
    fn foreach_rejects_unknown_container_and_dry_runs() {
        let mut core = core_with_bandolier();
        let _ = core.handle_dot_command(".foreach gem in knapsack; sell item");
        assert!(!core.foreach.is_running(), "unknown container must not start");

        // Dry run (no commands) lists matches without starting anything.
        let _ = core.handle_dot_command(".foreach in bandolier");
        assert!(!core.foreach.is_running());
        assert!(core.take_outbound().is_empty());
    }
}
