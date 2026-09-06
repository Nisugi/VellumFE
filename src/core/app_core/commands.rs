use anyhow::Result;

use super::AppCore;
use crate::data::{CommandOutcome, ShellZoneTarget, UiAction, ZoneOp};

/// Compass/vertical movement words — everything else in a room's wayto
/// edges is a "portal" (go door, climb stair, enter hole, ...).
const COMPASS_WORDS: [&str; 22] = [
    "north",
    "northeast",
    "east",
    "southeast",
    "south",
    "southwest",
    "west",
    "northwest",
    "up",
    "down",
    "out",
    "n",
    "ne",
    "e",
    "se",
    "s",
    "sw",
    "w",
    "nw",
    "u",
    "d",
    "o",
];

/// The GS "interesting" mapdb tags `.go2 targets` enumerates as a shop/guild
/// directory (Lich's `gs_interesting_tags`, go2.lic:1009). Resolution accepts
/// any mapdb tag; this is just the advertised directory vocabulary.
const INTERESTING_TAGS: [&str; 40] = [
    "alchemist",
    "armorshop",
    "bakery",
    "bank",
    "bardguild",
    "boutique",
    "chronomage",
    "clericguild",
    "clericshop",
    "cobbling",
    "collectibles",
    "consignment",
    "empathguild",
    "exchange",
    "fletcher",
    "forge",
    "furrier",
    "gemshop",
    "general store",
    "grocer",
    "herbalist",
    "inn",
    "locksmith",
    "mail",
    "npccleric",
    "npchealer",
    "pawnshop",
    "portmaster",
    "postoffice",
    "rangerguild",
    "smokeshop",
    "sorcererguild",
    "sunfist",
    "treasuremaster",
    "town",
    "voln",
    "warriorguild",
    "weaponshop",
    "wizardguild",
    "moneychanger",
];

/// Room-object nouns that look like walkable portals, for the fallback
/// when the mapdb doesn't know the current room.
const PORTAL_NOUNS: [&str; 18] = [
    "door", "gate", "arch", "archway", "portal", "stair", "stairs", "stairway", "steps", "ladder",
    "trapdoor", "opening", "entrance", "path", "trail", "bridge", "ramp", "curtain",
];

/// One pickable portal exit: what the user SEES (`label`, the movement) and
/// what a pick SENDS (`command`). For a plain string edge the two are the
/// same command; for a StringProc edge the label is the movement extracted
/// from the transpiled script and the command is `.go2 <destid>`, which
/// routes/walks the edge natively (P1) — the user never sees `.go2 <id>`.
#[derive(Clone, Debug, PartialEq)]
pub struct PortalCandidate {
    pub label: String,
    pub command: String,
}

/// Non-compass wayto exits of a room as labeled portal candidates, including
/// StringProc edges (shown by their movement). `db` resolves proc-edge
/// scripts to a movement label. Stable (BTreeMap) order, deduped by label.
fn portal_candidates(
    room: &crate::core::mapdb::Room,
    db: &crate::core::mapdb::MapDb,
) -> Vec<PortalCandidate> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (&dest, command) in &room.wayto {
        let trimmed = command.trim();
        if crate::core::mapdb::is_proc_command(trimmed) {
            // A StringProc edge: show the movement it performs, send a native
            // .go2 to that neighbor.
            //
            // When the proc doesn't transpile we still list the exit — the
            // router plans through any edge with a `timeto` regardless of
            // whether we can read its `wayto` (dijkstra.rs), and `.go2 <dest>`
            // walks it (handing off to Lich if it can't cross). Dropping it
            // here made real exits invisible to `.portal` while `.go2` could
            // reach them. We just can't name the MOVEMENT, so name the
            // DESTINATION instead; recognizer work upgrades these labels to
            // real movements with no change here.
            let label = match proc_edge_label(trimmed, db) {
                Some(label) => label,
                None => proc_edge_destination_label(dest, db),
            };
            if COMPASS_WORDS.contains(&label.to_ascii_lowercase().as_str()) {
                continue;
            }
            if seen.insert(label.to_ascii_lowercase()) {
                out.push(PortalCandidate {
                    label,
                    command: format!(".go2 {dest}"),
                });
            }
            continue;
        }
        if COMPASS_WORDS.contains(&trimmed.to_ascii_lowercase().as_str()) {
            continue;
        }
        if seen.insert(trimmed.to_ascii_lowercase()) {
            out.push(PortalCandidate {
                label: trimmed.to_string(),
                command: trimmed.to_string(),
            });
        }
    }
    out
}

/// The movement a StringProc edge performs, for display — the first
/// `Move`/`Put` target in its transpiled actions (e.g. "climb footpath").
/// `None` when the edge doesn't transpile to any movement.
fn proc_edge_label(command: &str, db: &crate::core::mapdb::MapDb) -> Option<String> {
    use crate::core::pathing::edge::WalkAction;
    let actions = crate::core::pathing::transpile::transpile_edge(db, command)?;
    actions.into_iter().find_map(|a| match a {
        WalkAction::Move(cmd) | WalkAction::Put(cmd) => Some(cmd),
        _ => None,
    })
}

/// Display label for a proc edge we can't transpile: where it goes, since we
/// can't say how it gets there. Prefers the destination's room title (already
/// bracketed in mapdb, e.g. `[Vornavis, Wooded Plains]`), then its location,
/// and falls back to the bare room id.
fn proc_edge_destination_label(dest: u32, db: &crate::core::mapdb::MapDb) -> String {
    if let Some(room) = db.room(dest) {
        if let Some(title) = room.title.first().filter(|t| !t.trim().is_empty()) {
            return title.trim().to_string();
        }
        if let Some(loc) = room.location.as_deref().filter(|l| !l.trim().is_empty()) {
            return loc.trim().to_string();
        }
    }
    format!("room {dest}")
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

/// Everything after the first (dot-command) word of `command`, verbatim, or
/// `None` when there is no argument. `command` is the whole line including the
/// leading `.`. Used where the argument must be taken literally (e.g.
/// `.testline`): slicing past the command word avoids matching a token that
/// happens to occur inside the command word itself.
fn command_rest(command: &str) -> Option<&str> {
    command[1..]
        .splitn(2, char::is_whitespace)
        .nth(1)
        .filter(|s| !s.is_empty())
}

/// Split a multi-command macro containing sleep segments into the part to
/// send immediately (joined by \r, the way multi-command strings always
/// ride to the server) and the paused remainder as (cumulative delay,
/// command) pairs. None when the string has no sleep segments — the
/// normal send path handles it untouched.
fn split_sleep_macro(text: &str) -> Option<(Option<String>, Vec<(std::time::Duration, String)>)> {
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

/// Minimal percent-encoding for a query-string value: keeps the RFC 3986
/// unreserved set (`A-Z a-z 0-9 - _ . ~`) and encodes everything else as
/// `%XX`. Enough for a character name in the `.webinfo` app deep link
/// without pulling in a URL crate; GemStone names are letters, but spaces
/// or punctuation in a session label stay safe.
fn percent_encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

impl AppCore {
    /// Send command to server
    pub fn send_command(&mut self, command: String) -> Result<CommandOutcome> {
        // A presentation target belongs only to the command currently being
        // resolved. Frontends either commit it after a successful network
        // enqueue or roll it back through `finish_game_command_send`.
        self.staged_goals_launch = None;
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

        // A hand-typed GOALS gets the native trainer too: the reply's
        // LaunchURL is claimed by the panel instead of the system browser.
        // `goals web` records the host browser as its presentation target.
        let trimmed = command.trim();
        if trimmed.eq_ignore_ascii_case("goals") {
            self.arm_skill_trainer();
        } else if trimmed.eq_ignore_ascii_case("goals web") {
            self.arm_goals_host_browser();
            return Ok(CommandOutcome::Game("goals".to_string()));
        }

        // Intercept game "quit" command - save settings before disconnecting
        // This handles the case where users close terminal after game disconnect
        if command.trim().eq_ignore_ascii_case("quit") {
            self.save_on_quit();
            // Don't set self.running = false - let VellumFE stay open
            // Fall through to send command to server
        }

        // Echo command to windows subscribed to "main" stream. Hidden
        // extended-feed requests (`_inventory manager/viewitem ...`) go out
        // silently, like Saga - a sync fires up to ten of them and probe
        // traffic would otherwise spam H> lines through combat.
        if !command.starts_with("_inventory ") {
            self.echo_command_to_main(&command);
        }

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
                custom_emoji: None,
                inline_image: None,
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
            custom_emoji: None,
            inline_image: None,
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
        let Some(endpoint) = self
            .message_processor
            .remote
            .as_ref()
            .and_then(|remote| remote.launch_endpoint())
        else {
            self.add_system_message("Web server is not running (bind failed or still starting)");
            return;
        };
        let port = endpoint.bound_port();
        let token = endpoint.token();
        let host = if self.config.web.bind == "127.0.0.1" {
            "127.0.0.1".to_string()
        } else {
            Self::local_lan_ip().unwrap_or_else(|| self.config.web.bind.clone())
        };
        let url = format!("http://{host}:{port}/#token={token}");
        // Deep link for the iOS/Android apps' Remote login tab: same
        // server, but paired through the app shell (token lands in the
        // phone's Keychain/Keystore instead of a browser). The character
        // name rides along (URL-encoded) so a scanned entry auto-names
        // itself in the app's character picker instead of showing host:port.
        let mut app_url = format!("vellum://remote?host={host}&port={port}&token={token}");
        if let Some(name) = self.config.character.as_deref().filter(|n| !n.is_empty()) {
            app_url.push_str(&format!("&name={}", percent_encode_query(name)));
        }
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
        let location = self
            .map
            .current_location
            .as_deref()
            .map(|key| self.map.display_name(key).to_owned())
            .unwrap_or_else(|| "?".into());
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

    /// The current room's portal exits as labeled candidates (Ask B): the
    /// mapdb room's non-compass wayto edges — string AND StringProc — UNIONED
    /// with portal-looking room-object nouns from the server, deduped by
    /// label. Shared by `.portal` and the dynamic `portals` controller wheel.
    pub fn portal_candidate_list(&self) -> Vec<PortalCandidate> {
        let mut out: Vec<PortalCandidate> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        // Mapdb wayto edges (string + StringProc).
        if let (Some(room_id), Some(db)) = (self.map.current_room_id, self.map.mapdb()) {
            if let Some(room) = db.room(room_id) {
                for c in portal_candidates(room, db) {
                    if seen.insert(c.label.to_ascii_lowercase()) {
                        out.push(c);
                    }
                }
            }
        }

        // Union in the server's portal-looking room-object nouns. These are
        // the host's noun exits (doors/gates/arches) — useful even when the
        // mapdb already knows the room, and the only source when it doesn't.
        for obj in &self.game_state.room_objects {
            if let Some(noun) = obj.noun.as_deref() {
                if PORTAL_NOUNS.contains(&noun.to_ascii_lowercase().as_str()) {
                    let command = format!("go {noun}");
                    if seen.insert(command.to_ascii_lowercase()) {
                        out.push(PortalCandidate {
                            label: command.clone(),
                            command,
                        });
                    }
                }
            }
        }
        out
    }

    /// Reachable interesting-tag destinations from the current room, as
    /// `(tag, room id, eta seconds)`, nearest first (Lich's `;go2 targets`
    /// directory). Empty when no mapdb / current room / reachable tags.
    pub fn go2_target_directory(&self) -> Vec<(String, u32, f64)> {
        let (Some(db), Some(from)) = (self.map.mapdb(), self.map.current_room_id) else {
            return Vec::new();
        };
        let mut found: Vec<(String, u32, f64)> = Vec::new();
        for &tag in INTERESTING_TAGS.iter() {
            if db.room_ids_with_tag(tag).is_empty() {
                continue;
            }
            if let Some(dest) = crate::core::pathing::find_nearest_by_tag(db, from, tag) {
                let eta = match crate::core::pathing::path_to(db, from, dest) {
                    Some(route) => {
                        let mut rooms = vec![from];
                        rooms.extend(&route);
                        crate::core::pathing::estimate_time(db, &rooms)
                    }
                    None if from == dest => 0.0,
                    None => continue,
                };
                found.push((tag.to_string(), dest, eta));
            }
        }
        found.sort_by(|a, b| a.2.total_cmp(&b.2));
        found
    }

    /// The current room's portal commands — back-compat flat command list for
    /// the controller wheel and phone snapshot. Prefer `portal_candidate_list`
    /// where labels matter.
    pub fn portal_commands(&self) -> Vec<String> {
        self.portal_candidate_list()
            .into_iter()
            .map(|c| c.command)
            .collect()
    }

    /// `.portal [n|text]` — walk through the room's non-compass exit
    /// ("go door", "climb stair", ...). One candidate: walk it. Several:
    /// list them; `.portal 2` or `.portal arch` picks. Candidates come
    /// from `portal_commands`. Returns the movement command to send
    /// upstream, or None.
    fn handle_portal_command(&mut self, args: &[String]) -> Option<String> {
        let mut candidates = self.portal_candidate_list();

        match (candidates.len(), args.first()) {
            (0, _) => {
                self.add_system_message("No portal found here.");
                None
            }
            (1, None) => Some(candidates.remove(0).command),
            (_, None) => {
                // Several portals: open a local popup menu — keyboard and
                // controller navigable on the desktop frontends. Phones
                // (whose menus are server-driven) get the listing message.
                // The menu SHOWS the movement label, RUNS the command.
                let items: Vec<crate::data::ui_state::PopupMenuItem> = candidates
                    .iter()
                    .map(|c| crate::data::ui_state::PopupMenuItem {
                        text: c.label.clone(),
                        command: c.command.clone(),
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
                    .map(|(i, c)| format!("{}) {}", i + 1, c.label))
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
                        return Some(candidates.remove(index - 1).command);
                    }
                }
                let needle = pick.to_ascii_lowercase();
                if let Some(found) = candidates
                    .iter()
                    .position(|c| c.label.to_ascii_lowercase().contains(&needle))
                {
                    return Some(candidates.remove(found).command);
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
            crate::core::uipack::PARTS
                .iter()
                .map(|s| s.to_string())
                .collect()
        };
        self.uiexport_pack(&name, &parts, None, extra_files);
    }

    /// Build a pack; shared by `.uiexport` and the pack editor panels
    /// (which pass an explicit destination folder). Returns success.
    pub fn uiexport_pack(
        &mut self,
        name: &str,
        parts: &[String],
        dest_dir: Option<std::path::PathBuf>,
        extra_files: Vec<(String, Vec<u8>)>,
    ) -> bool {
        let layout_toml = self.layout.clone().to_share_toml().ok();
        let base = match crate::config::Config::base_dir() {
            Ok(base) => base,
            Err(e) => {
                self.add_system_message(&format!("Export failed: {e:#}"));
                return false;
            }
        };
        let quickbars_toml = toml::to_string_pretty(&crate::core::uipack::QuickbarsFile {
            quickbars: self.config.quickbars.clone(),
        })
        .ok();
        let settings_toml = toml::to_string_pretty(
            &crate::core::uipack::ShareableSettings::from_config(&self.config),
        )
        .ok();
        let request = crate::core::uipack::ExportRequest {
            name,
            parts,
            character: self.config.character.as_deref(),
            layout_toml,
            // The live active-skin concept is gone (skins are presets);
            // packs no longer bundle a skin dir on export.
            active_skin: None,
            active_theme: Some(self.config.active_theme.as_str()),
            quickbars_toml,
            settings_toml,
            extra_files: &extra_files,
            dest_dir: dest_dir.as_deref(),
        };
        match crate::core::uipack::export(&base, &request) {
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
                true
            }
            Err(e) => {
                self.add_system_message(&format!("Export failed: {e:#}"));
                false
            }
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
                        "{} file(s). Run `.uiimport {} apply [parts...]` to install — replaced files are backed up.",
                        preview.entries.len(),
                        target
                    ));
                }
                Err(e) => self.add_system_message(&format!("Could not read pack: {e:#}")),
            }
            return None;
        }

        // `.uiimport <name> apply [parts...]` — extra args limit the
        // install to those parts.
        let selected: Option<Vec<String>> =
            (args.len() > 2).then(|| args[2..].iter().map(|s| s.to_lowercase()).collect());
        self.uiimport_apply(&path, selected.as_deref())
    }

    /// Install a pack from a resolved path; shared by `.uiimport ... apply`
    /// and the pack editor panels. `selected` limits to those parts.
    pub fn uiimport_apply(
        &mut self,
        path: &std::path::Path,
        selected: Option<&[String]>,
    ) -> Option<(String, Vec<u8>)> {
        let base = match crate::config::Config::base_dir() {
            Ok(base) => base,
            Err(e) => {
                self.add_system_message(&format!("Import failed: {e:#}"));
                return None;
            }
        };
        match crate::core::uipack::apply(&base, path, self.config.character.as_deref(), selected) {
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
                // Config-merging parts first, so the hot reloads below see
                // the merged state.
                if let Some(text) = &outcome.quickbars_toml {
                    match toml::from_str::<crate::core::uipack::QuickbarsFile>(text) {
                        Ok(file) => {
                            self.config.quickbars = file.quickbars;
                            let _ = self.save_config();
                            self.add_system_message("Quickbars installed.");
                        }
                        Err(e) => self
                            .add_system_message(&format!("Pack's quickbars did not parse: {e:#}")),
                    }
                }
                if let Some(text) = &outcome.settings_toml {
                    match toml::from_str::<crate::core::uipack::ShareableSettings>(text) {
                        Ok(settings) => {
                            settings.apply_to(&mut self.config);
                            let _ = self.save_config();
                            self.apply_tts_settings();
                            self.add_system_message(
                                "General settings installed (connection settings are never touched; some take effect on restart).",
                            );
                        }
                        Err(e) => self
                            .add_system_message(&format!("Pack's settings did not parse: {e:#}")),
                    }
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
                    Err(e) => self.add_system_message(&format!("Macros did not reload: {e:#}")),
                }
                if let Some(skin) = &outcome.skin {
                    // The pack's skin dir is extracted but never auto-set:
                    // skins are presets now — applying migrates it.
                    self.add_system_message(&format!(
                        "Skin '{skin}' included — apply it with .setskin {skin}"
                    ));
                }
                if let Some(theme) = &outcome.theme {
                    self.config.active_theme = theme.clone();
                    let _ = self.save_config();
                    self.add_system_message(&format!("Active theme set to '{theme}'."));
                }
                if let Some(layout) = &outcome.layout_name {
                    self.add_system_message(&format!(
                        "TUI layout installed — load it with .loadlayout {layout}"
                    ));
                }
                self.needs_render = true;
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

    /// `.exportskin <name>`: zip the current appearance's assignments plus
    /// the pool art they reference into `exports/<name>-skin.zip` — the
    /// same format `.jinx` installs, so the file is directly shareable
    /// (Discord, vellum-assets submission).
    fn export_skin_pack(&mut self, name: &str) {
        match crate::config::skin_pack::export(&self.config.appearance, name) {
            Ok((path, findings)) => {
                self.add_system_message(&format!("Skin pack written: {}", path.display()));
                for warning in &findings.warnings {
                    self.add_system_message(&format!("  warning: {warning}"));
                }
                self.add_system_message(
                    "Share the zip directly, or submit it to vellum-assets for .jinx install.",
                );
            }
            Err(e) => self.add_system_message(&format!("Export failed: {e:#}")),
        }
    }

    /// `.importskin <file>`: install a skin-pack zip — art into the pool
    /// (collisions renamed, convention art kept), assignments into the
    /// appearance store. The GUI syncs its live look via
    /// `appearance_changed_externally`.
    fn import_skin_pack(&mut self, arg: &str) {
        let Some(path) = Self::resolve_skin_pack_arg(arg) else {
            self.add_system_message(&format!(
                "No skin pack found for '{arg}' (tried the path itself, imports/ and exports/)"
            ));
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.add_system_message(&format!("Cannot read {}: {e}", path.display()));
                return;
            }
        };
        if !crate::config::skin_pack::is_pack_format(&bytes) {
            self.add_system_message(&format!(
                "{} is not a skin pack (legacy skin zips install via .jinx, or convert \
                 with `vellum-fe migrate-skin`)",
                path.display()
            ));
            return;
        }
        let pack = match crate::config::skin_pack::read_pack_bytes(&bytes) {
            Ok(pack) => pack,
            Err(e) => {
                self.add_system_message(&format!("Cannot read skin pack: {e}"));
                return;
            }
        };
        match crate::config::skin_pack::install_files(&pack) {
            Ok(report) => {
                report.assignments.apply_to(&mut self.config.appearance);
                let character = self.config.character.clone();
                if let Err(e) = self.config.appearance.save(character.as_deref()) {
                    self.add_system_message(&format!("Appearance not saved: {e:#}"));
                } else if character.is_some() {
                    let _ = self.config.appearance.save(None);
                }
                self.appearance_changed_externally = true;
                // Keep the pack re-appliable: persist its (rename-adjusted)
                // manifest as a preset so .setskin <name> works later.
                let mut manifest = pack.manifest.clone();
                manifest.assignments = report.assignments.clone();
                let preset_name = if manifest.meta.name.trim().is_empty() {
                    path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "imported".to_string())
                } else {
                    manifest.meta.name.clone()
                };
                if let Err(e) = crate::config::skin_pack::write_preset(&preset_name, &manifest) {
                    self.add_system_message(&format!("Preset not saved: {e:#}"));
                }
                self.add_system_message(&format!(
                    "Skin pack '{}' installed: {} file(s) added, {} identical, {} renamed, {} kept.",
                    pack.manifest.meta.name,
                    report.installed.len(),
                    report.identical.len(),
                    report.renamed.len(),
                    report.kept_existing.len()
                ));
                for (from, to) in &report.renamed {
                    self.add_system_message(&format!("  renamed: {from} -> {to}"));
                }
                for warning in &report.warnings {
                    self.add_system_message(&format!("  warning: {warning}"));
                }
                self.needs_render = true;
            }
            Err(e) => self.add_system_message(&format!("Install failed: {e:#}")),
        }
    }

    /// Resolve a `.importskin` argument: the path as given, else
    /// `<config>/imports/` then `<config>/exports/`, each tried verbatim
    /// and with the `-skin.zip` suffix `.exportskin` writes.
    fn resolve_skin_pack_arg(arg: &str) -> Option<std::path::PathBuf> {
        let direct = std::path::PathBuf::from(arg);
        if direct.is_file() {
            return Some(direct);
        }
        let base = crate::config::Config::base_dir().ok()?;
        for dir in ["imports", "exports"] {
            for name in [arg.to_string(), format!("{arg}-skin.zip"), format!("{arg}.zip")] {
                let candidate = base.join(dir).join(&name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
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
                if let Err(err) = self.tts_manager.speak_text_now(
                    "A giant rat scampers out of the shadows. Roundtime, 5 seconds.",
                ) {
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
        let JinxFlags {
            only_repo,
            force,
            dry_run,
        } = flags;

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
                    let ack = self.jinx_worker.start(Request::Search {
                        pattern: pattern.to_string(),
                    });
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
            "install" => match jinx_install_target(&pos) {
                Some((category, name)) => {
                    let ack = self.jinx_worker.start(Request::Install {
                        name,
                        category,
                        only_repo,
                        overwrite: force,
                    });
                    self.add_system_message(&ack);
                }
                None => self.add_system_message(
                    "[jinx] usage: .jinx install [<category>] <name> [--repo=<r>]",
                ),
            },
            "update" => match jinx_install_target(&pos) {
                // Update is install with overwrite; a bare `.jinx update`
                // updates everything (auto-update).
                Some((category, name)) => {
                    let ack = self.jinx_worker.start(Request::Install {
                        name,
                        category,
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
                        Ok(()) => {
                            self.add_system_message(&format!("[jinx] repo '{name}' -> {url}"))
                        }
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
            "  .jinx install [<category>] <name> [--force]   install an asset",
            "      e.g. .jinx install compass stormfront / .jinx install hands bone",
            "      names repeat across categories, so name one when asked",
            "  .jinx update [<category>] [<name>]  update one asset, or all if omitted",
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
                self.add_system_message(&format!(
                    "[map] checking {repo} for the latest release..."
                ));
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
                    "usage: .go2 <room id | uid | tag | saved name | text> - also: .go2 stop / status / save <name> [id] / targets / saved / back",
                );
            }
            "stop" => self.stop_travel(),
            "reload" => {
                // Force a fresh mapdb load (Lich's `;go2 reload`).
                self.map.reload();
                self.add_system_message("[go2] reloading the map database...");
            }
            "status" => {
                let status = match (
                    self.travel.task(),
                    self.map.mapdb(),
                    self.map.current_room_id,
                ) {
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
                    (Some(task), _, _) => {
                        format!("[go2] -> room {} (resolving...)", task.destination)
                    }
                    _ => "[go2] not traveling.".to_string(),
                };
                self.add_system_message(&status);
            }
            "save" => {
                let Some(name) = args.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                    self.add_system_message(
                        "usage: .go2 save <name> [room id] (defaults to the current room)",
                    );
                    return;
                };
                if name.parse::<u32>().is_ok() || name.eq_ignore_ascii_case("back") {
                    self.add_system_message(
                        "[go2] that name would shadow a room id or keyword - pick another",
                    );
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
            "saved" => {
                // The saved-target list (formerly what `.go2 targets` showed).
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
            "targets" => {
                // A directory of reachable shop/guild destinations from here,
                // built from the mapdb's tags (Lich's `;go2 targets`).
                if self.map.mapdb().is_none() {
                    self.add_system_message(
                        "[go2] map database not loaded - configure it in Settings > Map",
                    );
                    return;
                }
                if self.map.current_room_id.is_none() {
                    self.add_system_message(
                        "[go2] current room unknown - can't list reachable targets (see .room)",
                    );
                    return;
                }
                let found = self.go2_target_directory();
                if found.is_empty() {
                    self.add_system_message(
                        "[go2] no tagged destinations reachable from here (try .go2 saved)",
                    );
                    return;
                }
                self.add_system_message("[go2] reachable targets (nearest first):");
                for (tag, dest, eta) in found {
                    self.add_system_message(&format!(
                        "  .go2 {tag}  -> room {dest} ({})",
                        crate::core::travel::format_eta(eta)
                    ));
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
                    Some(&self.game_state.character),
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
                    let Some(container) = self.game_state.objects.find_container(query) else {
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
                                titles
                                    .iter()
                                    .take(12)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", ")
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
                    || (spec.status.registered.is_some() && c.status.registered.is_none());
                type_ok && unknown
            });
            if need_scan {
                if let Some(cmd) = self.message_processor.start_inventory_scan() {
                    // Inject the scan command into the outbound path (zero
                    // delay = next drain).
                    self.queue_timed_command(std::time::Duration::from_millis(0), cmd.to_string());
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

    /// Harmony parameters for this session: the stored recipe when it was
    /// generated against the current theme background (so a saved look stays
    /// re-tunable), else theme-derived defaults seeded from the most vivid
    /// theme swatch. Shared by `.harmony`, `.harmony skin`, and the GUI
    /// Generate tab's action handler.
    pub fn harmony_params(&self) -> crate::core::harmony::HarmonyParams {
        use crate::core::harmony::{HarmonyParams, Scheme};
        let theme = self.config.get_theme();
        let background = theme.background_primary.to_hex();
        // A stored recipe re-tunes the same look; ignore it once the theme
        // background changed, since its seed was chosen against the old one.
        let recipe = self
            .config
            .colors
            .harmony
            .clone()
            .filter(|r| r.background.eq_ignore_ascii_case(&background));
        let seed = recipe
            .as_ref()
            .map(|r| r.seed.clone())
            .or_else(|| theme.seed_swatches().into_iter().next())
            .unwrap_or_else(|| theme.link_color.to_hex());
        let defaults = HarmonyParams::default();
        HarmonyParams {
            seed,
            background,
            scheme: recipe
                .as_ref()
                .and_then(|r| Scheme::parse(&r.scheme))
                .unwrap_or(defaults.scheme),
            variance: recipe.as_ref().map_or(defaults.variance, |r| r.variance),
            min_contrast: recipe
                .as_ref()
                .map_or(defaults.min_contrast, |r| r.min_contrast),
            separation: recipe
                .as_ref()
                .map_or(defaults.separation, |r| r.separation),
            room_title_spread: recipe
                .as_ref()
                .map_or(defaults.room_title_spread, |r| r.room_title_spread),
            pins: recipe.as_ref().map(|r| r.pins.clone()).unwrap_or_default(),
        }
    }

    /// `.harmony [scheme|schemes]` — regenerate the game-text preset colors
    /// from the active theme with the harmony engine (`core::harmony`). The
    /// GUI Colors editor's Generate tab is the interactive version; this
    /// command gives the TUI the same engine with sensible defaults.
    fn handle_harmony(&mut self, args: &[String]) {
        use crate::config::{ColorConfig, HarmonyRecipe};
        use crate::core::harmony::{self, Scheme};

        if args
            .first()
            .is_some_and(|a| a.eq_ignore_ascii_case("schemes"))
        {
            self.add_system_message("=== Harmony schemes ===");
            for scheme in Scheme::ALL {
                self.add_system_message(&format!(
                    "  {:<14} {}",
                    scheme.name(),
                    scheme.description()
                ));
            }
            self.add_system_message(
                "Usage: .harmony [scheme] - regenerate preset colors from the active \
                 theme; .harmony skin <name> - write a matching skin",
            );
            return;
        }

        let mut params = self.harmony_params();
        if let Some(arg) = args.first() {
            match Scheme::parse(arg) {
                Some(scheme) => params.scheme = scheme,
                None => {
                    self.add_system_message(&format!(
                        "Unknown scheme '{}'. Try .harmony schemes for the list.",
                        arg
                    ));
                    return;
                }
            }
        }

        let result = harmony::generate(&params);
        let new_recipe = HarmonyRecipe {
            seed: params.seed.clone(),
            background: params.background.clone(),
            scheme: params.scheme.name().to_string(),
            variance: params.variance,
            min_contrast: params.min_contrast,
            separation: params.separation,
            room_title_spread: params.room_title_spread,
            pins: params.pins.clone(),
        };
        let character = self.config.character.clone();
        if let Err(err) = ColorConfig::persist_generated_presets(
            &result.colors,
            &result.room_bg,
            &result.prompts,
            &new_recipe,
            character.as_deref(),
        ) {
            self.add_system_message(&format!("Harmony generation failed to save: {}", err));
            return;
        }
        self.reload_colors();

        self.add_system_message(&format!(
            "=== Harmony: {} from seed {} on {} ===",
            params.scheme.name(),
            params.seed,
            params.background
        ));
        for (role, hex) in &result.colors {
            let contrast = harmony::wcag_contrast(hex, &params.background);
            let pinned = if params.pins.contains_key(role) {
                "  (pinned)"
            } else {
                ""
            };
            self.add_system_message(&format!(
                "  {:<17} {}  {:.1}:1{}",
                role, hex, contrast, pinned
            ));
        }
        let plate_contrast = result
            .color_for("roomName")
            .map(|room| harmony::wcag_contrast(room, &result.room_bg))
            .unwrap_or(1.0);
        self.add_system_message(&format!(
            "  {:<17} {}  {:.1}:1 vs room title (plate)",
            "roomName bg", result.room_bg, plate_contrast
        ));
        for (character, hex) in &result.prompts {
            let label = harmony::PROMPT_ROLES
                .iter()
                .find(|r| r.character == character)
                .map(|r| r.label)
                .unwrap_or("prompt");
            self.add_system_message(&format!(
                "  {:<17} {}  {:.1}:1  (prompt '{}')",
                label,
                hex,
                harmony::wcag_contrast(hex, &params.background),
                character
            ));
        }
        self.add_system_message(
            "Presets updated (previous colors.toml kept as .bak). \
             .harmony schemes lists schemes; the GUI Colors editor's Generate \
             tab offers seeds, pins, and preview.",
        );
    }

    /// `.spellwatch` - edit the missing-spells watch list.
    ///   .spellwatch add 606          one spell
    ///   .spellwatch add [101,103]    several
    ///   .spellwatch add all          everything currently active
    /// `.find <query>` - search the managed inventory snapshot (extended
    /// feed) by name and print where each match lives, closed containers
    /// flagged. Snapshot comes from `.invsync`; results are as fresh as the
    /// last sync.
    fn handle_find(&mut self, parts: &[&str]) {
        let query = parts[1..].join(" ").trim().to_ascii_lowercase();
        if query.is_empty() {
            self.add_system_message(
                "Usage: .find <name fragment> - searches the .invsync snapshot.",
            );
            return;
        }
        let Some(snapshot) = self.game_state.managed_inventory.as_ref() else {
            self.add_system_message("[find] no inventory snapshot yet - run .invsync first.");
            return;
        };
        let mut lines: Vec<String> = Vec::new();
        for item in &snapshot.items {
            let hay = item.name.to_ascii_lowercase();
            let hay_long = item
                .long
                .as_deref()
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if hay.contains(&query) || hay_long.contains(&query) {
                lines.push(format!(
                    "  {} - {}  (#{})",
                    item.name,
                    snapshot.location_of(item),
                    item.id
                ));
            }
        }
        let total = snapshot.items.len();
        let incomplete = !snapshot.complete;
        if lines.is_empty() {
            self.add_system_message(&format!(
                "[find] no '{query}' in the snapshot ({total} items{}).",
                if incomplete { ", INCOMPLETE" } else { "" }
            ));
            return;
        }
        let header = format!(
            "[find] {} match{}:",
            lines.len(),
            if lines.len() == 1 { "" } else { "es" }
        );
        self.add_system_message(&header);
        for line in lines {
            self.add_system_message(&line);
        }
        if incomplete {
            self.add_system_message("  (snapshot INCOMPLETE - rerun .invsync)");
        }
    }

    /// `.drag` - verified item moves (extended feed's `_drag` verb, each
    /// confirmed against `<left>/<right>` hand events within 8s).
    fn handle_drag(&mut self, parts: &[&str]) {
        const USAGE: &str = "Usage: .drag <exist> left|right|drop|wear|feet - or - \
                             .drag <exist> in|on|behind|underneath <dest-exist>";
        use crate::core::item_mover::MoveKind;
        let (Some(item), Some(what)) = (parts.get(1), parts.get(2)) else {
            self.add_system_message(USAGE);
            return;
        };
        let item = item.trim_start_matches('#').to_string();
        let kind = match what.to_ascii_lowercase().as_str() {
            "left" => MoveKind::ToLeftHand,
            "right" => MoveKind::ToRightHand,
            "drop" => MoveKind::Drop,
            "wear" => MoveKind::Wear,
            "feet" => MoveKind::PlaceFeet,
            rel @ ("in" | "on" | "behind" | "underneath") => {
                let Some(dest) = parts.get(3) else {
                    self.add_system_message(USAGE);
                    return;
                };
                let dest = dest.trim_start_matches('#').to_string();
                // Lockers and similar are addressed by their in_selector
                // noun phrase, when the managed snapshot knows one.
                let selector = self
                    .game_state
                    .managed_inventory
                    .as_ref()
                    .and_then(|s| s.items.iter().find(|i| i.id == dest))
                    .and_then(|i| i.in_selector.clone());
                MoveKind::PutIn {
                    dest,
                    relation: rel.to_string(),
                    selector,
                }
            }
            _ => {
                self.add_system_message(USAGE);
                return;
            }
        };
        let hands = crate::core::item_mover::HandsView {
            left: self
                .game_state
                .objects
                .hand(crate::core::game_objects::Hand::Left)
                .map(|i| i.id.clone()),
            right: self
                .game_state
                .objects
                .hand(crate::core::game_objects::Hand::Right)
                .map(|i| i.id.clone()),
        };
        let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
        match self.item_mover.start(&item, kind, &hands, now_ms) {
            Ok(()) => {}
            Err(reason) => self.add_system_message(&format!("[drag] refused: {reason}")),
        }
    }

    ///   .spellwatch rem 606 | [..] | all
    ///   .spellwatch (or list)        show the list and what's missing
    fn handle_spellwatch(&mut self, parts: &[&str]) {
        use crate::core::missing_spells;
        const USAGE: &str = "Usage: .spellwatch add|rem <number> | [n,n,...] | all - \
                             bare .spellwatch lists the watch list.";
        let verb = parts.get(1).map(|s| s.to_ascii_lowercase());
        // The spell argument may contain spaces ("[101, 103]"): rejoin.
        let arg = parts.get(2..).map(|rest| rest.concat()).unwrap_or_default();
        match verb.as_deref() {
            None | Some("list") => {
                let watched = self.game_state.character.watched_spells.clone();
                if watched.is_empty() {
                    self.add_system_message(
                        "No spells watched. .spellwatch add <number> (or 'all') to start; \
                         add a Missing Spells window to see gaps at a glance.",
                    );
                    return;
                }
                let missing = missing_spells::missing(&self.game_state);
                self.add_system_message(&format!("Watched spells ({} missing):", missing.len()));
                for &number in &watched {
                    let name = missing_spells::spell_display_name(&self.game_state, number);
                    let state = if missing.iter().any(|m| m.number == number) {
                        "MISSING"
                    } else {
                        "active"
                    };
                    self.add_system_message(&format!("  {:>5}  {:<30} {}", number, name, state));
                }
            }
            Some("add") if arg.eq_ignore_ascii_case("all") => {
                let numbers = missing_spells::active_numbers(&self.game_state);
                if numbers.is_empty() {
                    self.add_system_message("Nothing active in ActiveSpells/Buffs to add.");
                    return;
                }
                let added = self.game_state.character.watch_spells(&numbers);
                self.add_system_message(&format!(
                    "Watching {} active spell{} ({} new).",
                    numbers.len(),
                    if numbers.len() == 1 { "" } else { "s" },
                    added
                ));
                self.needs_render = true;
            }
            Some("rem") | Some("remove") if arg.eq_ignore_ascii_case("all") => {
                let removed = self.game_state.character.unwatch_all_spells();
                self.add_system_message(&format!(
                    "Cleared {} watched spell{}.",
                    removed,
                    if removed == 1 { "" } else { "s" }
                ));
                self.needs_render = true;
            }
            Some("add") => match missing_spells::parse_spell_list(&arg) {
                Some(numbers) => {
                    let added = self.game_state.character.watch_spells(&numbers);
                    self.add_system_message(&format!(
                        "Added {} spell{} to the watch list ({} total).",
                        added,
                        if added == 1 { "" } else { "s" },
                        self.game_state.character.watched_spells.len()
                    ));
                    self.needs_render = true;
                }
                None => self.add_system_message(USAGE),
            },
            Some("rem") | Some("remove") => match missing_spells::parse_spell_list(&arg) {
                Some(numbers) => {
                    let removed = self.game_state.character.unwatch_spells(&numbers);
                    self.add_system_message(&format!(
                        "Removed {} spell{} from the watch list ({} left).",
                        removed,
                        if removed == 1 { "" } else { "s" },
                        self.game_state.character.watched_spells.len()
                    ));
                    self.needs_render = true;
                }
                None => self.add_system_message(USAGE),
            },
            Some(_) => self.add_system_message(USAGE),
        }
    }

    /// `.bestiary` — creature lookup against the bundled codex. Output is
    /// styled lines on the `bestiary` stream (falls back to main when no
    /// window subscribes to it); links re-enter this dispatch as
    /// `.bestiary ...` direct commands.
    fn handle_bestiary(&mut self, parts: &[&str]) {
        use crate::core::bestiary::format;
        let db = format::shared();
        if db.is_empty() {
            self.add_system_message("[bestiary] bundled codex failed to load.");
            return;
        }
        let args = &parts[1..];
        let lines = match args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
            None | Some("help") => format::help_lines(),
            Some("here") => match self.map.current_uid() {
                Some(uid) if uid > 0 => {
                    let rows = db.here(uid as u64);
                    format::table_lines(&rows, "Spawns around this room")
                }
                _ => {
                    self.add_system_message("[bestiary] current room uid unknown.");
                    return;
                }
            },
            Some("level") => {
                let spec = args.get(1).copied().unwrap_or("");
                let (lo, hi) = match spec.split_once('-') {
                    Some((a, b)) => (a.trim().parse().ok(), b.trim().parse().ok()),
                    None => {
                        let v: Option<i64> = spec.trim().parse().ok();
                        (v, v)
                    }
                };
                match (lo, hi) {
                    (Some(lo), Some(hi)) => {
                        format::table_lines(&db.by_level(lo, hi), &format!("Level {spec}"))
                    }
                    _ => {
                        self.add_system_message("Usage: .bestiary level <n> or <a>-<b>");
                        return;
                    }
                }
            }
            Some("area") => {
                let q = args[1..].join(" ");
                format::table_lines(&db.by_area(&q), &format!("Area '{q}'"))
            }
            Some("family") => {
                let q = args[1..].join(" ");
                format::table_lines(&db.by_family(&q), &format!("Family '{q}'"))
            }
            // `.bestiary rooms <map>|<lo>-<hi>[,<lo>-<hi>…]` — expand one
            // spawn table into per-room lines. Unlike every other page,
            // this APPENDS below the current card (no clear): it's a
            // drill-down of what's on screen, not a navigation step.
            Some("rooms") => {
                let raw = args[1..].join(" ");
                let (map_name, spec) = raw.split_once('|').unwrap_or(("?", raw.as_str()));
                let ranges: Vec<(u64, u64)> = spec
                    .split(',')
                    .filter_map(|r| {
                        let (lo, hi) = r.split_once('-')?;
                        Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?))
                    })
                    .filter(|(lo, hi)| lo <= hi)
                    .collect();
                if ranges.is_empty() {
                    self.add_system_message("Usage: .bestiary rooms <map>|<lo>-<hi>[,…]");
                    return;
                }
                let mapdb = self.map.mapdb().cloned();
                let lines = format::rooms_lines(map_name, &ranges, |uid| {
                    mapdb.as_ref().and_then(|db| db.room_id_of_uid(uid as i64))
                });
                self.add_client_lines_to_stream(format::STREAM, lines);
                return;
            }
            Some("undead") => format::table_lines(&db.undead(), "Undead"),
            Some("search") => {
                let q = args[1..].join(" ");
                format::table_lines(&db.search(&q), &format!("Search '{q}'"))
            }
            Some(_) => {
                let q = args.join(" ");
                match db.resolve(&q) {
                    Some(entry) => format::entry_lines(entry),
                    None => {
                        let rows = db.search(&q);
                        if rows.is_empty() {
                            self.add_system_message(&format!(
                                "[bestiary] no creature matches '{q}'."
                            ));
                            return;
                        }
                        format::table_lines(&rows, &format!("Matches for '{q}'"))
                    }
                }
            }
        };
        // App-style navigation: each step is a page, not appended scroll.
        // Only bestiary-subscribed windows clear; the main-window fallback
        // (no bestiary window in the layout) keeps its scrollback.
        self.clear_stream_windows(format::STREAM);
        self.add_client_lines_to_stream(format::STREAM, lines);
    }

    fn handle_dot_command(&mut self, command: &str) -> Result<CommandOutcome> {
        let parts: Vec<&str> = command[1..].split_whitespace().collect();
        let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        tracing::debug!("handle_dot_command: '{}'", command);

        match cmd.as_str() {
            // === COMMAND ARMS BEGIN === (command_help.rs tripwire: every
            // top-level arm literal here must have a help-table row, and
            // vice versa; the test extracts them from this source span)
            // Application commands
            "quit" | "q" => {
                // Keep-open mode (desktop default): first .quit detaches from
                // the game/Lich but leaves the window and scrollback up; a
                // second .quit — the connection is already down — falls
                // through to the real exit. `.exit` always closes in one step.
                if self.detach_quit_supported
                    && self.config.ui.keep_open_on_quit
                    && self.game_state.connected
                {
                    self.save_on_quit();
                    self.disconnect_requested = true;
                    self.add_system_message(
                        "Detached — window stays open. .reconnect or .launch <character> to resume; .quit again or .exit to close.",
                    );
                } else {
                    self.quit();
                }
            }
            "exit" => {
                self.quit();
            }
            "help" | "h" | "?" => {
                self.show_help();
            }
            "version" | "ver" => {
                self.show_version();
            }

            // Native skill trainer: opens the cached panel, or sends GOALS
            // to the game and captures the LaunchURL reply for a fresh page.
            // `.goals refresh` always re-fetches; `.goals web` records the
            // host browser as the LaunchURL target (the original play.net
            // web manager).
            "goals" => {
                let arg = parts.get(1).map(|s| s.to_lowercase());
                if arg.as_deref() == Some("web") {
                    self.arm_goals_host_browser();
                    return Ok(CommandOutcome::Game("goals".to_string()));
                }
                let refresh = arg.as_deref() == Some("refresh")
                    || self.ui_state.skill_trainer.data.is_none();
                if refresh {
                    let cmd = self.skill_trainer_reload_command();
                    return Ok(CommandOutcome::Game(cmd));
                }
                self.ui_state.skill_trainer.open = true;
            }

            // Re-establish a dropped game connection. Core can't reach the
            // socket task, so this hands off to the frontend runtime, which
            // owns the network channels (Direct re-auths; Lich re-attaches).
            "reconnect" => {
                return Ok(CommandOutcome::Ui(UiAction::Reconnect));
            }

            // SSH launcher: cold-start a headless Lich on the home PC over the
            // tunnel, then attach. `.launch <character>` runs it; bare `.launch`
            // (or `.launcher`) opens the editor. Core can't SSH or touch the
            // socket task, so both hand off to the frontend runtime.
            "launch" => {
                let character = parts[1..].join(" ");
                if character.trim().is_empty() {
                    return Ok(CommandOutcome::Ui(UiAction::LauncherEditor));
                }
                return Ok(CommandOutcome::Ui(UiAction::Launch(character)));
            }
            "launcher" => {
                return Ok(CommandOutcome::Ui(UiAction::LauncherEditor));
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
                            if self.config.sorter.enabled {
                                "on"
                            } else {
                                "off"
                            },
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
                    Err(e) => self
                        .add_system_message(&format!("Sorter toggle saved to session only: {e}")),
                }
            }

            // Creature-field camera/solver overrides (blanket + per-scene,
            // global/creature_field.toml) — the editor is the whole
            // workflow, so the command just opens it.
            "creaturefield" | "fieldcamera" => {
                return Ok(CommandOutcome::Ui(UiAction::CreatureFieldEdit));
            }

            "roomimages" | "roomimg" => {
                return self.handle_room_images_command(&parts);
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

            // Promote personal map edits into the staging export that gets
            // committed as defaults/map_overrides.json — the path from "I
            // dragged this map into shape" to "everyone's map looks like
            // this". Whole-map: promoted maps move out of the personal file.
            "mappromote" => {
                let arg = parts.get(1).map(|s| s.to_string());
                let key = match arg.as_deref() {
                    None => match self.map.current_location.clone() {
                        Some(key) => Some(key),
                        None => {
                            self.add_system_message(
                                "No current map; .mappromote <map-key> or .mappromote all",
                            );
                            return Ok(CommandOutcome::Handled);
                        }
                    },
                    Some("all") => None,
                    Some(key) => Some(key.to_owned()),
                };
                match self.map.promote_overrides(key.as_deref()) {
                    Ok((promoted, path)) => {
                        self.add_system_message(&format!(
                            "Promoted {} map(s) to {}: {}",
                            promoted.len(),
                            path.display(),
                            promoted.join(", ")
                        ));
                        self.add_system_message(
                            "Ship it: merge that file into defaults/map_overrides.json and commit.",
                        );
                    }
                    Err(e) => self.add_system_message(&format!("mappromote: {e}")),
                }
            }

            // Asset manager (the native jinx client): download and update
            // skins, icon maps, layouts, and game data from federated repos.
            // Network work runs off-thread (see jinx_worker); repo edits are
            // instant and inline.
            "jinx" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                // `.jinx gui` opens the native asset panel (GUI-only); every
                // other subcommand runs inline / off-thread here.
                if args.first().map(|s| s.as_str()) == Some("gui") {
                    return Ok(CommandOutcome::Ui(UiAction::JinxPanel));
                }
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
                                format!(", {} incompatible regex(es) skipped", data.skipped.len())
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
                                    // Game-data files are named exactly and
                                    // share no namespace with set art.
                                    category: None,
                                    only_repo: None,
                                    overwrite: true,
                                },
                            );
                            self.add_system_message(&ack);
                        }
                        None => self.add_system_message(
                            "Usage: .data update <name> (e.g. gameobj-data.xml)",
                        ),
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
                        self.config.macros_local = crate::config::MacrosConfig::load_local(
                            self.config.character.as_deref(),
                        )
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

            // Shareable UI packs: capability hooks too — the GUI adds /
            // installs its live layout alongside the core pack, the TUI
            // and headless run the plain core pack.
            "uiexport" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                return Ok(CommandOutcome::Ui(UiAction::UiExport(args)));
            }
            "uiimport" => {
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                return Ok(CommandOutcome::Ui(UiAction::UiImport(args)));
            }
            // The guided panel over .uiexport/.uiimport (desktop frontends).
            "packs" | "packeditor" => {
                return Ok(CommandOutcome::Ui(UiAction::PackEditor));
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

            // Toggling is core-side (the overlay window lives in ui_state,
            // shared by every frontend); `dump` is a UiAction so each
            // frontend can append its own report sections.
            "performance" | "perf" => {
                if parts.get(1).map(|s| s.eq_ignore_ascii_case("dump")) == Some(true) {
                    return Ok(CommandOutcome::Ui(UiAction::PerformanceDump));
                }
                let enabled = self.toggle_performance_overlay();
                self.add_system_message(if enabled {
                    "Performance monitor shown (.performance again to hide, .performance dump for a report)."
                } else {
                    "Performance monitor hidden."
                });
                return Ok(CommandOutcome::Handled);
            }

            // Layout commands are capability hooks (parity plan D3): core
            // owns the command names, each frontend owns its persistence
            // model — TOML cell layouts in the TUI, window-snapshot
            // checkpoints in the GUI, a "needs the desktop client"
            // answer on phones.
            "savelayout" => {
                return Ok(CommandOutcome::Ui(UiAction::SaveLayout(
                    parts.get(1).map(|name| name.to_string()),
                )));
            }
            "loadlayout" => {
                // `.loadlayout <name> [--keep-skin]` — the flag keeps the
                // loader's appearance (skin/theme/art) and takes only the
                // arrangement. Lenient spellings: --keep-skin / --keep-skins /
                // --keep_my_skins / --keepskin all normalize the same.
                let mut name = None;
                let mut keep_skin = false;
                for part in &parts[1..] {
                    let normalized: String = part
                        .trim_start_matches('-')
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .collect::<String>()
                        .to_ascii_lowercase();
                    if part.starts_with('-')
                        && matches!(
                            normalized.as_str(),
                            "keepskin" | "keepskins" | "keepmyskins"
                        )
                    {
                        keep_skin = true;
                    } else if name.is_none() {
                        name = Some(part.to_string());
                    }
                }
                // The flag is meaningless without a name (bare form lists).
                let keep_skin = keep_skin && name.is_some();
                return Ok(CommandOutcome::Ui(UiAction::LoadLayout { name, keep_skin }));
            }
            "layouts" => {
                return Ok(CommandOutcome::Ui(UiAction::ListLayouts));
            }
            "resize" => {
                // `.resize [name]` — bare refits to the current size; a name
                // adopts just that saved layout's geometry (GUI).
                return Ok(CommandOutcome::Ui(UiAction::ResizeLayout(
                    parts.get(1).map(|name| name.to_string()),
                )));
            }
            "anchorinfer" => {
                // One-shot: synthesize snap anchors from flush edges (GUI).
                return Ok(CommandOutcome::Ui(UiAction::AnchorInfer));
            }
            // Bake the current GUI appearance into a skin. Core knows the
            // command so the TUI can answer "GUI-only" instead of
            // "Unknown command".
            "saveskin" => match parts.get(1) {
                Some(name) => {
                    return Ok(CommandOutcome::Ui(UiAction::SaveSkin(name.to_string())));
                }
                None => {
                    self.add_system_message(
                        "Usage: .saveskin <name> — bakes the current appearance \
                         (doll, compass, status icons, frames, backgrounds) into a skin.",
                    );
                }
            },

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
                Some(page) => return Ok(CommandOutcome::Ui(UiAction::WebUiOpen(page.to_string()))),
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
            "spellwatch" => {
                self.handle_spellwatch(&parts);
            }
            "find" => {
                self.handle_find(&parts);
            }
            "emptyhands" | "eh" => {
                // Lich's empty_hands as a native command: stow both hands
                // (Lich's per-hand cascade), remember the stack for
                // .fillhands. Same StashTask travel uses.
                if self.hand_stash.is_some() {
                    self.add_system_message("[hands] a stow/retrieve is already running.");
                } else if let Some(owner) = self.automation_blocked_by("hands") {
                    self.add_system_message(&format!(
                        "[hands] {} is driving - .stop it first.",
                        owner.desc
                    ));
                } else {
                    self.hand_stash = Some(crate::core::travel::stash::StashTask::empty());
                }
            }
            "fillhands" | "fh" => {
                if self.hand_stash.is_some() {
                    self.add_system_message("[hands] a stow/retrieve is already running.");
                } else if let Some(owner) = self.automation_blocked_by("hands") {
                    self.add_system_message(&format!(
                        "[hands] {} is driving - .stop it first.",
                        owner.desc
                    ));
                } else if self.hand_stash_stack.is_empty() {
                    self.add_system_message(
                        "[hands] nothing remembered - .emptyhands stows and remembers first.",
                    );
                } else {
                    let stack = std::mem::take(&mut self.hand_stash_stack);
                    self.hand_stash = Some(crate::core::travel::stash::StashTask::fill(stack));
                }
            }
            "viewitem" | "inspect" => {
                // Item detail over the extended feed: parsed look/read
                // sections echo to main and feed the GUI inspector panel.
                let Some(exist) = parts.get(1) else {
                    self.add_system_message("Usage: .viewitem <exist-id>");
                    return Ok(CommandOutcome::Handled);
                };
                let exist = exist.trim_start_matches('#').to_string();
                let via = self
                    .game_state
                    .managed_inventory
                    .as_ref()
                    .and_then(|s| s.via_selector_for(&exist));
                let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                self.message_processor
                    .inv_service
                    .request_view(&exist, via.as_deref(), now_ms);
            }
            "drag" => {
                // Verified item moves over the extended feed's _drag verb:
                //   .drag <exist> left|right|drop|wear|feet
                //   .drag <exist> in|on|behind|underneath <dest-exist>
                self.handle_drag(&parts);
            }
            "invsync" => {
                // Refresh the extended feed's structured inventory snapshot
                // (`_inventory manager` + continuation-following). Direct-mode
                // WRAYTH banner required for the server to answer.
                if self.message_processor.inv_service.loading() {
                    self.add_system_message("[invsync] refresh already in progress.");
                } else {
                    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
                    self.message_processor.inv_service.request_refresh(now_ms);
                    self.add_system_message("[invsync] requesting inventory snapshot...");
                }
            }
            "bestiary" => {
                self.handle_bestiary(&parts);
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
            "alertpacks" => {
                // Bare `.alertpacks` opens the browser where one exists; with
                // a subcommand it stays text-driven, so the whole workflow
                // remains available in the TUI and over the bridge.
                if parts.len() == 1 {
                    return Ok(CommandOutcome::Ui(UiAction::AlertPacks));
                }
                self.handle_alertpacks_command(&parts);
            }
            "edithighlight" | "edithl" => {
                return Ok(CommandOutcome::Ui(UiAction::EditHighlight(
                    parts.get(1).map(|name| name.to_string()),
                )));
            }
            "testline" => {
                // Everything after the command word, verbatim. See
                // `command_rest`: slicing past the word avoids the old
                // `command.find(first_token)` bug where the token matched
                // INSIDE "testline" and injected a polluted line — defeating
                // the very tool used to verify highlight/squelch patterns.
                match command_rest(command) {
                    Some(text) => self.inject_test_line(text),
                    None => self.add_system_message("Usage: .testline <text>"),
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
            // Indicator template builder: create/edit every status indicator,
            // its conditions, and condition-driven icons in one place.
            "indicators" | "indicator" => {
                return Ok(CommandOutcome::Ui(UiAction::EditIndicators));
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
            "harmony" => {
                if parts.get(1).is_some_and(|s| s.eq_ignore_ascii_case("skin")) {
                    match parts.get(2) {
                        Some(name) => {
                            return Ok(CommandOutcome::Ui(UiAction::HarmonySkin(name.to_string())));
                        }
                        None => self.add_system_message(
                            "Usage: .harmony skin <name> - write a skin (panel + frame \
                             images) rendered from the harmony recipe",
                        ),
                    }
                } else {
                    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                    self.handle_harmony(&args);
                }
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
            "exportskin" => {
                if let Some(name) = parts.get(1) {
                    self.export_skin_pack(name);
                } else {
                    self.add_system_message(
                        "Usage: .exportskin <name> - zip the current appearance + its art \
                         into a shareable skin pack",
                    );
                }
            }
            "importskin" => {
                if let Some(arg) = parts.get(1) {
                    self.import_skin_pack(arg);
                } else {
                    self.add_system_message(
                        "Usage: .importskin <file> - install a skin pack zip \
                         (looks in imports/ and exports/, or give a full path)",
                    );
                }
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
                        return Ok(CommandOutcome::Ui(UiAction::Zone {
                            zone,
                            op: ZoneOp::On,
                        }));
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
                let new_state =
                    forced.unwrap_or_else(|| !self.layout.windows.iter().any(|w| w.base().locked));
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

            // === COMMAND ARMS END ===
            _ => {
                self.add_system_message(&format!("Unknown command: {}", command));
                self.add_system_message("Type .help for list of commands");
            }
        }

        // Command input is now managed by the CommandInput widget

        // Don't send anything to server
        Ok(CommandOutcome::Handled)
    }

    /// `.roomimages [on|off|set <image>|clear|list|edit]` — room art by uid.
    ///
    /// `set`/`clear` act on the room the character is standing in, which is
    /// the whole point: the user never types a uid.
    fn handle_room_images_command(&mut self, parts: &[&str]) -> Result<CommandOutcome> {
        let sub = parts.get(1).map(|s| s.to_lowercase());
        match sub.as_deref() {
            None => {
                let state = if self.config.room_images.enabled {
                    "on"
                } else {
                    "off"
                };
                let uid_now = self.message_processor.current_room_uid();
                let store = self.room_images_store().clone();
                let mapped: usize = store.images.iter().map(|i| i.rooms.len()).sum();
                let here = uid_now.and_then(|uid| {
                    store
                        .images
                        .iter()
                        .find(|i| i.rooms.contains(&uid))
                        .map(|i| (uid, i.name.clone()))
                });
                self.add_system_message(&format!("Room images {state} — {mapped} room(s) mapped."));
                match here {
                    Some((uid, image)) => {
                        self.add_system_message(&format!("This room ({uid}) shows '{image}'."))
                    }
                    None => match uid_now {
                        Some(uid) => self.add_system_message(&format!(
                            "This room ({uid}) has no art. Use .roomimages set <image>."
                        )),
                        None => self
                            .add_system_message("Room id unknown yet — move once, then try again."),
                    },
                }
                self.add_system_message("Usage: .roomimages [on|off|set <image>|clear|list|edit]");
            }
            Some("on") | Some("off") => {
                let target = sub.as_deref() == Some("on");
                self.config.room_images.enabled = target;
                self.message_processor.set_room_images_enabled(target);
                let note = match self.save_config() {
                    Ok(()) => String::new(),
                    Err(e) => format!(" (session only: {e})"),
                };
                self.add_system_message(&format!(
                    "Room images {}{note}.",
                    if target { "on" } else { "off" }
                ));
            }
            Some("edit") => {
                return Ok(CommandOutcome::Ui(UiAction::RoomImagesEdit));
            }
            Some("list") => {
                let store = self.room_images_store().clone();
                if store.images.is_empty() {
                    self.add_system_message("No room art mapped yet.");
                } else {
                    self.add_system_message("Room art:");
                    for entry in &store.images {
                        let rooms: Vec<String> =
                            entry.rooms.iter().map(|r| r.to_string()).collect();
                        self.add_system_message(&format!(
                            "  {} -> {}",
                            entry.name,
                            if rooms.is_empty() {
                                "(no rooms)".to_string()
                            } else {
                                rooms.join(", ")
                            }
                        ));
                    }
                }
            }
            Some("set") => {
                let Some(image) = parts.get(2) else {
                    self.add_system_message("Usage: .roomimages set <image>");
                    return Ok(CommandOutcome::Handled);
                };
                let image = image.to_string();
                let Some(uid) = self.message_processor.current_room_uid() else {
                    self.add_system_message("Room id unknown yet — move once, then try again.");
                    return Ok(CommandOutcome::Handled);
                };
                if !crate::core::inline_image::contains(&image) {
                    self.add_system_message(&format!(
                        "No image named '{image}' in global/images/inline. \
                         Add the file and run .reload."
                    ));
                    return Ok(CommandOutcome::Handled);
                }

                let mut store = self.room_images_store().clone();
                // A room belongs to exactly one image: drop it from any other
                // entry first so `set` MOVES rather than silently duplicating.
                let moved_from = store
                    .images
                    .iter_mut()
                    .find(|i| i.name != image && i.rooms.contains(&uid))
                    .map(|i| {
                        i.rooms.retain(|r| *r != uid);
                        i.name.clone()
                    });
                match store.images.iter_mut().find(|i| i.name == image) {
                    Some(entry) => {
                        if !entry.rooms.contains(&uid) {
                            entry.rooms.push(uid);
                        }
                    }
                    None => store.images.push(crate::config::room_images::RoomImageDef {
                        name: image.clone(),
                        rooms: vec![uid],
                        rows: None,
                        align: None,
                        variants: Vec::new(),
                    }),
                }
                if let Some(name) = self
                    .game_state
                    .room_name
                    .clone()
                    .or_else(|| self.room_subtitle.clone())
                {
                    store.names.insert(uid.to_string(), name);
                }
                self.commit_room_images(store);
                match moved_from {
                    Some(old) => self.add_system_message(&format!(
                        "Room {uid} moved from '{old}' to '{image}'."
                    )),
                    None => self.add_system_message(&format!("Room {uid} now shows '{image}'.")),
                }
                if !self.config.room_images.enabled {
                    self.add_system_message(
                        "Note: room images are off — turn on with .roomimages on.",
                    );
                }
            }
            Some("clear") => {
                let Some(uid) = self.message_processor.current_room_uid() else {
                    self.add_system_message("Room id unknown yet — move once, then try again.");
                    return Ok(CommandOutcome::Handled);
                };
                let mut store = self.room_images_store().clone();
                let removed = store
                    .images
                    .iter_mut()
                    .find(|i| i.rooms.contains(&uid))
                    .map(|i| {
                        i.rooms.retain(|r| *r != uid);
                        i.name.clone()
                    });
                match removed {
                    Some(name) => {
                        store.names.remove(&uid.to_string());
                        self.commit_room_images(store);
                        self.add_system_message(&format!("Room {uid} no longer shows '{name}'."));
                    }
                    None => self.add_system_message(&format!("Room {uid} has no art mapped.")),
                }
            }
            Some(other) => {
                self.add_system_message(&format!(
                    "Unknown .roomimages option '{other}'. \
                     Usage: .roomimages [on|off|set <image>|clear|list|edit]"
                ));
            }
        }
        Ok(CommandOutcome::Handled)
    }

    /// The loaded room-art mappings, loading them on first use.
    pub fn room_images_store(&mut self) -> &crate::config::room_images::RoomImagesConfig {
        if self.room_images.is_none() {
            self.room_images = Some(
                crate::config::Config::load_room_images(self.config.character.as_deref())
                    .unwrap_or_default(),
            );
        }
        self.room_images.as_ref().expect("just loaded")
    }

    /// Persist mappings, republish the lookup index, and keep the in-memory
    /// copy in sync so the next command sees the change.
    ///
    /// The store is the merged global+character view, so it is saved through the
    /// scope-splitting writer — flattening it into the global file would leak
    /// character entries there and lose edits to them on the next launch.
    pub fn commit_room_images(&mut self, store: crate::config::room_images::RoomImagesConfig) {
        use crate::config::room_images::RoomImageIndex;
        self.message_processor
            .set_room_image_index(RoomImageIndex::build(&store));
        if let Err(e) =
            crate::config::Config::save_room_images_split(&store, self.config.character.as_deref())
        {
            self.add_system_message(&format!("Room art saved to session only: {e}"));
        }
        self.room_images = Some(store);
        // Re-render the room so the change is visible without walking out.
        self.room_window_dirty = true;
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

/// Split `install`/`update` positionals into `(category, name)`.
///
/// Asset names are not unique across categories — a `stealthblue` compass
/// set and a `stealthblue.vellumpack` skin both exist — so the category can
/// be given as its own word: `.jinx install compass stealthblue`. Two
/// positionals means category-then-name; one means a bare name, which
/// resolution accepts when it's unambiguous and rejects (listing the
/// choices) when it isn't.
///
/// A free function so it's unit-testable without an `AppCore`.
fn jinx_install_target(pos: &[&str]) -> Option<(Option<String>, String)> {
    match (pos.get(1), pos.get(2)) {
        (Some(category), Some(name)) => Some((Some(category.to_string()), name.to_string())),
        (Some(name), None) => Some((None, name.to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod jinx_install_target_tests {
    use super::jinx_install_target;

    #[test]
    fn splits_category_and_name() {
        // `.jinx install compass stealthblue` — category then name.
        let pos = ["install", "compass", "stealthblue"];
        assert_eq!(
            jinx_install_target(&pos),
            Some((Some("compass".to_string()), "stealthblue".to_string()))
        );

        // A bare name stays a bare name; resolution decides if it's ambiguous.
        let pos = ["install", "stealthblue"];
        assert_eq!(
            jinx_install_target(&pos),
            Some((None, "stealthblue".to_string()))
        );

        // `.jinx install` alone has no target — the caller prints usage.
        assert_eq!(jinx_install_target(&["install"]), None);
    }
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
        core.ui_state
            .windows
            .insert("main".to_string(), main_window);
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
    fn candidates_skip_compass_dedupe_and_surface_procs_by_movement() {
        // A room whose edges mix cardinals, string portals, and StringProc
        // edges. Compass words are excluded; a transpilable proc edge appears
        // by its movement label ("climb footpath"); an un-interpretable proc
        // edge is STILL listed, labeled by where it goes.
        let db = crate::core::mapdb::MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Road]"],
                 "wayto": {
                    "2": "north",
                    "3": "go door",
                    "4": "climb stair",
                    "5": ";e empty_hands; move 'climb footpath'; fill_hands",
                    "6": ";e some_unparseable_confluence_thing"
                 },
                 "timeto": {"2": 0.2, "3": 0.2, "4": 0.2, "5": 0.2, "6": 0.2},
                 "paths": ""},
                {"id": 2, "uid": [9000002], "location": "T", "title": ["[N]"],
                 "wayto": {"1": "south"}, "timeto": {"1": 0.2}, "paths": ""},
                {"id": 6, "uid": [9000006], "location": "T",
                 "title": ["[Vornavis, Wooded Plains]"],
                 "wayto": {"1": "out"}, "timeto": {"1": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let room = db.room(1).unwrap();
        let got = portal_candidates(room, &db);
        assert_eq!(
            got,
            vec![
                PortalCandidate {
                    label: "go door".into(),
                    command: "go door".into()
                },
                PortalCandidate {
                    label: "climb stair".into(),
                    command: "climb stair".into()
                },
                // The proc footpath shows its movement, runs .go2 to room 5.
                PortalCandidate {
                    label: "climb footpath".into(),
                    command: ".go2 5".into()
                },
                // The un-interpretable proc is a REAL exit the router can plan
                // through, so it stays listed - named by its destination.
                PortalCandidate {
                    label: "[Vornavis, Wooded Plains]".into(),
                    command: ".go2 6".into(),
                },
            ],
            "compass excluded, string edges verbatim, proc edges by movement or destination"
        );
    }

    #[test]
    fn untranspilable_proc_edge_to_an_unknown_room_still_lists() {
        // Same rescue when the destination isn't in the mapdb either: the edge
        // is still walkable via .go2, so it must not vanish from .portal.
        let db = crate::core::mapdb::MapDb::from_json(
            r#"[
                {"id": 1, "uid": [9000001], "location": "T", "title": ["[Road]"],
                 "wayto": {"7": ";e some_unparseable_confluence_thing"},
                 "timeto": {"7": 0.2}, "paths": ""}
            ]"#,
        )
        .unwrap();
        let got = portal_candidates(db.room(1).unwrap(), &db);
        assert_eq!(
            got,
            vec![PortalCandidate {
                label: "room 7".into(),
                command: ".go2 7".into()
            }],
            "falls back to the bare room id rather than dropping the exit"
        );
    }

    /// Keep-open `.quit` (desktop default): first `.quit` while connected
    /// detaches (flag for the runtime) and keeps the app running; a second
    /// `.quit` — now disconnected — exits; `.exit` always exits.
    #[test]
    fn quit_keeps_window_open_then_second_quit_exits() {
        let mut core = AppCore::new_for_test();
        core.detach_quit_supported = true;
        core.game_state.connected = true;
        assert!(core.config.ui.keep_open_on_quit, "keep-open is the default");

        core.send_command(".quit".to_string());
        assert!(core.disconnect_requested, "first .quit requests detach");
        assert!(core.running, "app stays up after first .quit");

        // Runtime drained the request and closed the socket.
        core.disconnect_requested = false;
        core.game_state.connected = false;

        core.send_command(".quit".to_string());
        assert!(!core.running, "second .quit exits");
    }

    #[test]
    fn exit_always_exits_even_while_connected() {
        let mut core = AppCore::new_for_test();
        core.detach_quit_supported = true;
        core.game_state.connected = true;
        core.send_command(".exit".to_string());
        assert!(!core.running);
        assert!(!core.disconnect_requested);
    }

    /// Toggle off restores the old behavior: `.quit` closes immediately.
    #[test]
    fn quit_exits_immediately_when_keep_open_disabled() {
        let mut core = AppCore::new_for_test();
        core.detach_quit_supported = true;
        core.game_state.connected = true;
        core.config.ui.keep_open_on_quit = false;
        core.send_command(".quit".to_string());
        assert!(!core.running);
        assert!(!core.disconnect_requested);
    }

    /// Frontends that don't drain disconnect_requested (headless/web) keep
    /// today's `.quit` semantics — otherwise a phone `.quit` would set a flag
    /// nobody reads and become a no-op.
    #[test]
    fn quit_exits_when_frontend_lacks_detach_support() {
        let mut core = AppCore::new_for_test();
        core.game_state.connected = true;
        assert!(!core.detach_quit_supported, "headless default");
        core.send_command(".quit".to_string());
        assert!(!core.running);
        assert!(!core.disconnect_requested);
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
    fn go2_reload_drops_and_rekicks_the_mapdb() {
        let mut core = AppCore::new_for_test();
        core.map.set_mapdb_for_test(
            crate::core::mapdb::MapDb::from_json(
                r#"[{"id": 1, "uid": [9000001], "location": "T", "title": ["[A]"],
                     "wayto": {}, "timeto": {}, "paths": ""}]"#,
            )
            .unwrap(),
        );
        assert!(core.map.mapdb().is_some());
        core.handle_go2(&["reload".to_string()]);
        // With no configured source, reload drops the (test-injected) db and
        // finds nothing to load — proving it re-kicked rather than no-oped.
        assert!(core.map.mapdb().is_none(), "reload dropped the db");
    }

    #[test]
    fn go2_targets_lists_reachable_tagged_destinations_nearest_first() {
        let mut core = AppCore::new_for_test();
        // From room 1: a near bank (0.2) and a far pawnshop (5.0).
        core.map.set_mapdb_for_test(
            crate::core::mapdb::MapDb::from_json(
                r#"[
                    {"id": 1, "uid": [9000001], "location": "Town", "title": ["[Square]"],
                     "wayto": {"2": "east", "3": "west"}, "timeto": {"2": 0.2, "3": 5.0},
                     "paths": ""},
                    {"id": 2, "uid": [9000002], "location": "Town", "title": ["[Bank]"],
                     "tags": ["bank"], "wayto": {"1": "west"}, "timeto": {"1": 0.2}, "paths": ""},
                    {"id": 3, "uid": [9000003], "location": "Town", "title": ["[Pawnshop]"],
                     "tags": ["pawnshop"], "wayto": {"1": "east"}, "timeto": {"1": 5.0},
                     "paths": ""}
                ]"#,
            )
            .unwrap(),
        );
        core.map.current_room_id = Some(1);

        let dir = core.go2_target_directory();
        let tags: Vec<&str> = dir.iter().map(|(t, _, _)| t.as_str()).collect();
        assert!(tags.contains(&"bank"), "lists the bank: {tags:?}");
        assert!(tags.contains(&"pawnshop"), "lists the pawnshop: {tags:?}");
        // Bank (0.2) is nearer than pawnshop (5.0), so it lists first.
        let bank_i = tags.iter().position(|t| *t == "bank").unwrap();
        let pawn_i = tags.iter().position(|t| *t == "pawnshop").unwrap();
        assert!(bank_i < pawn_i, "nearest first: {tags:?}");
        assert_eq!(dir[bank_i].1, 2, "bank resolves to room 2");
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

        assert_eq!(
            core.wheel_pick_command("portals", &[1]),
            Some("go arch".into())
        );
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
        assert_eq!(
            core.wheel_pick_command("spells", &[0]),
            Some("prep 101".into())
        );
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
        let menu = core
            .ui_state
            .popup_menu
            .as_ref()
            .expect("portal picker menu");
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
    use super::{command_rest, sleep_segment_seconds, split_sleep_macro};
    use std::time::Duration;

    #[test]
    fn command_rest_slices_past_the_command_word_not_a_substring_match() {
        // The bug: `command.find("test")` matched inside ".testline" itself, so
        // `.testline test rat` injected "testline test rat". command_rest
        // slices past the word.
        assert_eq!(
            command_rest(".testline test rat appears"),
            Some("test rat appears")
        );
        assert_eq!(command_rest(".testline line two"), Some("line two"));
        assert_eq!(command_rest(".testline t"), Some("t"));
        // Leading/interior arguments kept verbatim (no trimming of the rest).
        assert_eq!(
            command_rest(".testline  double space"),
            Some(" double space")
        );
        // No argument.
        assert_eq!(command_rest(".testline"), None);
        assert_eq!(command_rest(".testline "), None);
    }

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
    fn percent_encode_query_keeps_unreserved_and_escapes_rest() {
        // Plain GemStone names pass through untouched.
        assert_eq!(super::percent_encode_query("Rysk"), "Rysk");
        // Spaces and punctuation in a session label are escaped.
        assert_eq!(super::percent_encode_query("Ma Roon"), "Ma%20Roon");
        assert_eq!(super::percent_encode_query("a&b=c"), "a%26b%3Dc");
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
            (
                ".editwindow main",
                UiAction::EditWindow(Some("main".into())),
            ),
            (".editwindow", UiAction::EditWindow(None)),
            (
                ".hidewindow main",
                UiAction::HideWindow(Some("main".into())),
            ),
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
            (".creaturefield", UiAction::CreatureFieldEdit),
            (".fieldcamera", UiAction::CreatureFieldEdit),
            (".reconnect", UiAction::Reconnect),
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
    fn loadlayout_parses_keep_skin_flag() {
        // `.loadlayout <name> [--keep-skin]` — lenient flag spellings, any
        // argument order; the flag is meaningless without a name.
        let cases: Vec<(&str, Option<&str>, bool)> = vec![
            (".loadlayout combat", Some("combat"), false),
            (".loadlayout combat --keep-skin", Some("combat"), true),
            (".loadlayout --keep-skin combat", Some("combat"), true),
            (".loadlayout combat --keep_my_skins", Some("combat"), true),
            (".loadlayout combat --keepskin", Some("combat"), true),
            (".loadlayout combat --keep-skins", Some("combat"), true),
            (".loadlayout", None, false),
            (".loadlayout --keep-skin", None, false), // flag without a name is dropped
        ];
        for (command, name, keep_skin) in cases {
            assert_eq!(
                ui_outcome(command),
                CommandOutcome::Ui(UiAction::LoadLayout {
                    name: name.map(str::to_string),
                    keep_skin,
                }),
                "wrong outcome for {command}"
            );
        }
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
        objects.register_container(
            "77".to_string(),
            "Bandolier".to_string(),
            Some("#77".to_string()),
        );
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
            objects.add_container_item("stow", GameItem::new("333", "crystal", "quartz crystal"));
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
            o.register_container(
                "77".to_string(),
                "Bandolier".to_string(),
                Some("#77".to_string()),
            );
            o.add_container_item("77", GameItem::new("101", "crystal", "quartz crystal"));
            o.add_container_item(
                "77",
                GameItem::new("102", "crystal", "smoky quartz crystal"),
            );
        }

        // Status unknown → the filter triggers an INVENTORY FULL scan and
        // defers, rather than running on incomplete data.
        let _ = core.handle_dot_command(".foreach marked gem in bandolier; get item");
        assert!(!core.foreach.is_running(), "deferred pending scan");
        assert_eq!(core.take_outbound(), vec!["inventory full".to_string()]);

        // Simulate the scan result landing on the registry.
        core.game_state.objects.set_status(
            "101".to_string(),
            ItemStatus {
                marked: Some(true),
                registered: Some(false),
            },
        );
        core.game_state.objects.set_status(
            "102".to_string(),
            ItemStatus {
                marked: Some(false),
                registered: Some(false),
            },
        );

        // Re-run: now status is known, only the marked gem runs.
        let _ = core.handle_dot_command(".foreach marked gem in bandolier; get item");
        assert!(core.foreach.is_running());
        assert_eq!(core.take_outbound(), vec!["get #101".to_string()]);
    }

    #[test]
    fn emptyhands_stows_then_fillhands_replays() {
        use crate::core::game_objects::{GameItem, Hand};
        let mut core = core_with_bandolier();
        core.game_state.objects.set_hand(
            Hand::Right,
            Some(GameItem::new("777", "sword", "a short sword")),
        );

        let _ = core.handle_dot_command(".emptyhands");
        assert!(core.hand_stash.is_some());
        core.tick_hand_stash();
        let sent = core.take_outbound();
        assert_eq!(sent.len(), 1, "one stow command for the held item");
        assert!(
            sent[0].contains("#777"),
            "targets the held item: {}",
            sent[0]
        );

        // The hand clearing confirms the stow; the task finishes and the
        // stack remembers the item for .fillhands.
        core.game_state.objects.set_hand(Hand::Right, None);
        core.tick_hand_stash();
        assert!(core.hand_stash.is_none(), "empty finished");
        assert_eq!(core.hand_stash_stack.len(), 1);

        let _ = core.handle_dot_command(".fillhands");
        assert!(core.hand_stash.is_some());
        core.tick_hand_stash();
        let sent = core.take_outbound();
        assert_eq!(sent, vec!["get #777".to_string()]);

        // Item back in hand completes the fill and clears the memory.
        core.game_state.objects.set_hand(
            Hand::Right,
            Some(GameItem::new("777", "sword", "a short sword")),
        );
        core.tick_hand_stash();
        assert!(core.hand_stash.is_none(), "fill finished");
        assert!(core.hand_stash_stack.is_empty());

        // Nothing remembered now: .fillhands refuses without starting.
        let _ = core.handle_dot_command(".fillhands");
        assert!(core.hand_stash.is_none());
    }

    #[test]
    fn foreach_rejects_unknown_container_and_dry_runs() {
        let mut core = core_with_bandolier();
        let _ = core.handle_dot_command(".foreach gem in knapsack; sell item");
        assert!(
            !core.foreach.is_running(),
            "unknown container must not start"
        );

        // Dry run (no commands) lists matches without starting anything.
        let _ = core.handle_dot_command(".foreach in bandolier");
        assert!(!core.foreach.is_running());
        assert!(core.take_outbound().is_empty());
    }
}

#[cfg(test)]
mod room_images_command_tests {
    use crate::core::AppCore;
    use crate::data::{CommandOutcome, UiAction};

    /// Guards a test: holds the registry + config-dir locks and redirects
    /// VELLUM_FE_DIR to a scratch dir. Without the redirect these tests
    /// WRITE ROOM MAPPINGS INTO THE USER'S REAL ~/.vellum-fe.
    struct ArtTestEnv {
        _art: std::sync::MutexGuard<'static, ()>,
        _dir_lock: std::sync::MutexGuard<'static, ()>,
        _dir: tempfile::TempDir,
    }

    impl Drop for ArtTestEnv {
        fn drop(&mut self) {
            std::env::remove_var("VELLUM_FE_DIR");
        }
    }

    /// Install one image in the pool so `.roomimages set` accepts it.
    fn install_art(name: &str) -> ArtTestEnv {
        use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
        let guard = crate::core::inline_image::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir_lock = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        let mut registry = CustomEmojiRegistry::default();
        registry.insert_for_test(CustomEmoji {
            name: name.to_string(),
            path: std::path::PathBuf::from(format!("{name}.png")),
            format: EmojiFormat::Png,
        });
        crate::core::inline_image::set_for_test(registry);
        ArtTestEnv {
            _art: guard,
            _dir_lock: dir_lock,
            _dir: dir,
        }
    }

    /// Put the core "in" a room the way <nav rm=> does.
    fn enter(core: &mut AppCore, uid: &str) {
        core.message_processor.process_element(
            &crate::parser::ParsedElement::RoomId {
                id: uid.to_string(),
            },
            &mut core.game_state,
            &mut core.ui_state,
            &mut core.room_components,
            &mut core.current_room_component,
            &mut core.room_window_dirty,
            &mut core.nav_room_id,
            &mut core.lich_room_id,
            &mut core.room_subtitle,
            None,
        );
    }

    fn mapped_rooms(core: &mut AppCore, image: &str) -> Vec<u64> {
        core.room_images_store()
            .images
            .iter()
            .find(|i| i.name == image)
            .map(|i| i.rooms.clone())
            .unwrap_or_default()
    }

    /// `.roomimages set` maps the room the character is standing in, so the
    /// user never types a uid.
    #[test]
    fn set_maps_the_current_room() {
        let _guard = install_art("pier");
        let mut core = AppCore::new_for_test();
        enter(&mut core, "7118245");

        let _ = core.send_command(".roomimages set pier".to_string());
        assert_eq!(mapped_rooms(&mut core, "pier"), vec![7118245]);
    }

    /// Setting a room that already belongs to another image MOVES it, rather
    /// than leaving a duplicate the format would permit but the loader would
    /// have to arbitrate.
    #[test]
    fn set_moves_a_room_between_images() {
        let _guard = install_art("pier");
        let mut core = AppCore::new_for_test();
        enter(&mut core, "7118245");
        let _ = core.send_command(".roomimages set pier".to_string());

        // A second image, also installed.
        {
            use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
            let mut registry = CustomEmojiRegistry::default();
            for name in ["pier", "dock"] {
                registry.insert_for_test(CustomEmoji {
                    name: name.into(),
                    path: std::path::PathBuf::from(format!("{name}.png")),
                    format: EmojiFormat::Png,
                });
            }
            crate::core::inline_image::set_for_test(registry);
        }
        let _ = core.send_command(".roomimages set dock".to_string());

        assert_eq!(mapped_rooms(&mut core, "dock"), vec![7118245]);
        assert!(
            mapped_rooms(&mut core, "pier").is_empty(),
            "the room must not stay mapped to both images"
        );
    }

    /// `.roomimages clear` unmaps the current room.
    #[test]
    fn clear_unmaps_the_current_room() {
        let _guard = install_art("pier");
        let mut core = AppCore::new_for_test();
        enter(&mut core, "7118245");
        let _ = core.send_command(".roomimages set pier".to_string());
        let _ = core.send_command(".roomimages clear".to_string());
        assert!(mapped_rooms(&mut core, "pier").is_empty());
    }

    /// Naming art that isn't installed is refused with a message rather than
    /// writing a mapping that could never render.
    #[test]
    fn set_refuses_an_uninstalled_image() {
        let _guard = install_art("pier");
        let mut core = AppCore::new_for_test();
        enter(&mut core, "7118245");
        let _ = core.send_command(".roomimages set nope".to_string());
        assert!(mapped_rooms(&mut core, "nope").is_empty());
    }

    /// Before any <nav rm=>, the client does not know where it is; `set` must
    /// say so instead of mapping room 0 or panicking.
    #[test]
    fn set_without_a_known_room_is_refused() {
        let _guard = install_art("pier");
        let mut core = AppCore::new_for_test();
        let _ = core.send_command(".roomimages set pier".to_string());
        assert!(mapped_rooms(&mut core, "pier").is_empty());
    }

    #[test]
    fn toggle_flips_the_setting() {
        let _guard = install_art("pier");
        let mut core = AppCore::new_for_test();
        assert!(!core.config.room_images.enabled, "off by default");
        let _ = core.send_command(".roomimages on".to_string());
        assert!(core.config.room_images.enabled);
        let _ = core.send_command(".roomimages off".to_string());
        assert!(!core.config.room_images.enabled);
    }

    #[test]
    fn edit_returns_its_ui_action() {
        let mut core = AppCore::new_for_test();
        let outcome = core
            .send_command(".roomimages edit".to_string())
            .expect("command should not error");
        assert_eq!(outcome, CommandOutcome::Ui(UiAction::RoomImagesEdit));
    }
}
