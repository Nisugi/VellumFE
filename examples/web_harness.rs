//! Dev harness: serves the real phone client with scripted macros and no
//! game connection, so the web UI can be driven in a browser.
//!
//!     cargo run --example web_harness
//!     open http://127.0.0.1:8399/play#token=abc123
//!
//! Simulates the runtime's macro handling: MacroSave/MacroDelete events
//! update an in-memory overlay and re-broadcast definitions, exactly like
//! AppCore::apply_macro_save. Cmd/Macro events are logged to stdout.

use vellum_fe::config::{MacroButton, MacrosConfig};
use vellum_fe::core::remote::{RemoteEvent, RemoteSessionInfo, RemoteSink, SessionState};
use vellum_fe::data::widget::{StyledLine, TextSegment};
use vellum_fe::frontend::web::server;

const TOKEN: &str = "abc123";

#[tokio::main]
async fn main() {
    let (mut sink, handles, mut event_rx) = RemoteSink::new(500);

    // --login: act like the headless (mobile) runtime with no session, so
    // the login overlay (and its Remote tab shell plumbing) can be driven.
    // SessionConnect walks the scripted state machine (authenticating →
    // connecting → connected); typing "drop" simulates a mid-session drop
    // (reconnecting → connected); SessionDisconnect returns to idle.
    let login_mode = std::env::args().any(|a| a == "--login");
    if login_mode {
        sink.set_session_control(true);
    }

    let base: MacrosConfig = toml::from_str(
        r##"
        [[group]]
        name = "Words"

        [[group.button]]
        label = "Look"
        command = "look"

        [[group.button]]
        label = "go"
        command = "go"
        insert = true

        [[group.button]]
        label = "second"
        command = "second"
        insert = true

        [[group.button]]
        label = "door"
        command = "door\r"
        insert = true
        color = "#d9b44f"

        [[group.button]]
        label = "places"

        [[group.button.option]]
        label = "to the bank"
        command = ";go2 bank"

        [[group.button.option]]
        label = "gate (word)"
        command = "gate"
        insert = true
        "##,
    )
    .expect("base macros parse");
    let mut local = MacrosConfig::default();
    sink.set_macros(&MacrosConfig::merge(base.clone(), local.clone()));

    sink.push_text(
        "main",
        std::sync::Arc::new(StyledLine {
            segments: vec![TextSegment::plain("[web_harness] ready — no game connected]")],
            stream: "main".to_string(),
            timestamp: None,
        }),
    );

    // Scripted map scene: a small fake town so the map overlay can be
    // driven with no game and no mapdb — a street, a side lane, a labeled
    // connector, a stub pair, an entrance dot, and a ghost sketch.
    {
        use vellum_fe::core::remote::{
            RemoteGhostEdge, RemoteGhostNode, RemoteMapEdge, RemoteMapLabel, RemoteMapRoom,
            RemoteMapScene, RemoteMapSceneRef, RemoteMapState, RemoteStateSnapshot,
        };
        let mut rooms = Vec::new();
        let mut edges = Vec::new();
        let edge = |x1, y1, x2, y2, k, l: Option<&str>, ar, br| RemoteMapEdge {
            x1,
            y1,
            x2,
            y2,
            k,
            l: l.map(str::to_owned),
            ar,
            br,
        };
        for i in 0..5i32 {
            rooms.push(RemoteMapRoom {
                i: 100 + i as u32,
                x: i,
                y: 0,
                e: i == 2,
            });
            if i > 0 {
                edges.push(edge(i - 1, 0, i, 0, 0, None, None, None));
            }
        }
        for i in 0..3i32 {
            rooms.push(RemoteMapRoom {
                i: 200 + i as u32,
                x: 2,
                y: i + 1,
                e: false,
            });
            edges.push(edge(2, i, 2, i + 1, 0, None, None, None));
        }
        rooms.push(RemoteMapRoom { i: 300, x: 7, y: 0, e: false });
        edges.push(edge(4, 0, 7, 0, 1, Some("go gate"), None, None));
        rooms.push(RemoteMapRoom { i: 400, x: 0, y: 6, e: false });
        edges.push(edge(0, 0, 0, 6, 2, None, Some(100), Some(400)));
        let scene = std::sync::Arc::new(RemoteMapScene {
            location: "Harness Town".into(),
            sheet: "outdoor".into(),
            rooms,
            edges,
            labels: vec![RemoteMapLabel { x: 2, y: 1, t: "The Grid".into() }],
        });
        let mut snap = RemoteStateSnapshot::default();
        // Scripted wounds/scars so the status drawer's injury doll has
        // something to draw: wounds 1-3, a scar, and the two "no spot on
        // the silhouette" parts (back, nsys).
        for (part, level) in [
            ("head", 2u8),
            ("chest", 3),
            ("leftArm", 1),
            ("rightHand", 5),
            ("back", 6),
            ("nsys", 4),
        ] {
            snap.injuries.insert(part.to_string(), level);
        }
        snap.map_scene = RemoteMapSceneRef(Some(scene));
        snap.map_state = RemoteMapState {
            available: true,
            location: Some("Harness Town".into()),
            room: Some(102),
            cell: Some([2, 0]),
            in_ghost: false,
            // Scripted trip in progress: exercises the banner + Stop button.
            travel: Some(vellum_fe::core::remote::RemoteTravelStatus {
                dest: 300,
                done: 12,
                total: 47,
                eta: "1:04".into(),
            }),
            ghosts: vec![RemoteGhostNode { x: 3, y: -1, cur: false }],
            ghost_edges: vec![RemoteGhostEdge {
                x1: 3,
                y1: 0,
                x2: 3,
                y2: -1,
                l: Some("go shop".into()),
            }],
        };
        sink.flush_state(snap);
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8399")
        .await
        .expect("bind 8399");
    println!("harness: http://127.0.0.1:8399/play#token={TOKEN}");
    tokio::spawn(async move {
        let _ = server::serve_listener_with_token(listener, handles, TOKEN.to_string()).await;
    });

    let session = |state, character: &Option<String>| RemoteSessionInfo {
        state,
        character: character.clone(),
        ..Default::default()
    };

    // Scripted settings catalog for the phone "Client settings" sheet: one
    // entry of every kind (plus a sensitive redacted row and character-only
    // rows), same wire shape as settings_catalog_json. Puts validate
    // against the kind (range / options / JSON type) like the registry
    // setters do, and update the stored value so a re-open shows the edit.
    let mut settings_catalog = serde_json::json!([
        {
            "key": "ui.echo_commands",
            "label": "Echo commands",
            "category": "UI",
            "description": "Show commands you send in the main window.",
            "kind": { "type": "bool" },
            "scope": "global_or_character",
            "sensitive": false,
            "value": true
        },
        {
            "key": "ui.buffer_size",
            "label": "Scrollback lines",
            "category": "UI",
            "description": "Lines kept per text window.",
            "kind": { "type": "int", "min": 100, "max": 10000 },
            "scope": "global_or_character",
            "sensitive": false,
            "value": 5000
        },
        {
            "key": "ui.timestamp_position",
            "label": "Timestamps",
            "category": "UI",
            "description": "Where line timestamps are drawn.",
            "kind": { "type": "enum", "options": ["off", "left", "right"] },
            "scope": "global_or_character",
            "sensitive": false,
            "value": "left"
        },
        {
            "key": "sound.volume",
            "label": "Alert volume",
            "category": "Sound",
            "description": "Playback volume for highlight sounds.",
            "kind": { "type": "float", "min": 0.0, "max": 1.0 },
            "scope": "global_or_character",
            "sensitive": false,
            "value": 0.7
        },
        {
            "key": "tts.voice",
            "label": "Speech voice",
            "category": "Speech",
            "description": "Host TTS voice (empty = system default).",
            "kind": { "type": "optional_text" },
            "scope": "global_or_character",
            "sensitive": false,
            "value": ""
        },
        {
            "key": "tts.gags",
            "label": "Speech gags",
            "category": "Speech",
            "description": "Lines matching these are never spoken.",
            "kind": { "type": "list" },
            "scope": "global_or_character",
            "sensitive": false,
            "value": ["a chill wind blows"]
        },
        {
            "key": "connection.account",
            "label": "Account",
            "category": "Connection",
            "description": "Play.net account name.",
            "kind": { "type": "text" },
            "scope": "character_only",
            "sensitive": false,
            "value": "HARNESS"
        },
        {
            "key": "connection.password",
            "label": "Password",
            "category": "Connection",
            "description": "Stored account password.",
            "kind": { "type": "text" },
            "scope": "character_only",
            "sensitive": true,
            "value": "",
            "redacted": true
        },
        // Roaming phone prefs (settings Phase 5 slice B). Ship them SET
        // so the client's connect-time fetch visibly overrides local
        // prefs: story text 18px, OLED-black theme, thoughts chip in
        // front. Change any of the three on the phone and the put lands
        // here (character scope); re-open Client settings to see it.
        {
            "key": "web.story_size",
            "label": "Phone Text Size",
            "category": "Web",
            "description": "Phone story text size in px; roams with the character (0 = unset, phone keeps its own)",
            "kind": { "type": "int", "min": 0, "max": 32 },
            "scope": "global_or_character",
            "sensitive": false,
            "value": 18
        },
        {
            "key": "web.theme",
            "label": "Phone Theme",
            "category": "Web",
            "description": "Phone theme preset; roams with the character (empty = unset, phone keeps its own)",
            "kind": { "type": "optional_text" },
            "scope": "global_or_character",
            "sensitive": false,
            "value": "black"
        },
        {
            "key": "web.chip_order",
            "label": "Phone Chip Order",
            "category": "Web",
            "description": "Phone stream-chip order, leftmost first; roams with the character (empty = unset)",
            "kind": { "type": "list" },
            "scope": "global_or_character",
            "sensitive": false,
            "value": ["thoughts", "main"]
        }
    ]);

    while let Some(event) = event_rx.recv().await {
        match event {
            RemoteEvent::Command(text) if login_mode && text == "drop" => {
                println!("EVENT cmd: {text:?} (scripted drop → reconnect)");
                let character = Some("Harness".to_string());
                sink.set_session_state(session(SessionState::Reconnecting, &character));
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                sink.set_session_state(session(SessionState::Connected, &character));
            }
            RemoteEvent::SessionConnect { character, .. } if login_mode => {
                println!("EVENT session_connect: character={character:?}");
                let character = character.or_else(|| Some("Harness".to_string()));
                for state in [SessionState::Authenticating, SessionState::Connecting] {
                    sink.set_session_state(session(state, &character));
                    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                }
                sink.set_session_state(session(SessionState::Connected, &character));
            }
            RemoteEvent::SessionDisconnect if login_mode => {
                println!("EVENT session_disconnect → idle");
                sink.set_session_state(RemoteSessionInfo::default());
            }
            RemoteEvent::MapLocations {
                client_id,
                request_id,
            } => {
                println!("EVENT map_locations");
                sink.push_map_locations(
                    client_id,
                    request_id,
                    vec![
                        "Harness Town".into(),
                        "Harness Docks".into(),
                        "Empty Location".into(),
                    ],
                );
            }
            RemoteEvent::MapView {
                client_id,
                request_id,
                location,
            } => {
                use vellum_fe::core::remote::{RemoteMapEdge, RemoteMapRoom, RemoteMapScene};
                println!("EVENT map_view: {location:?}");
                if location == "Harness Docks" {
                    let rooms = (0..4i32)
                        .map(|i| RemoteMapRoom {
                            i: 500 + i as u32,
                            x: i,
                            y: i,
                            e: false,
                        })
                        .collect();
                    let edges = (1..4i32)
                        .map(|i| RemoteMapEdge {
                            x1: i - 1,
                            y1: i - 1,
                            x2: i,
                            y2: i,
                            k: 0,
                            l: None,
                            ar: None,
                            br: None,
                        })
                        .collect();
                    sink.push_map_browse(
                        client_id,
                        request_id,
                        location.clone(),
                        Some(std::sync::Arc::new(RemoteMapScene {
                            location,
                            sheet: "outdoor".into(),
                            rooms,
                            edges,
                            labels: vec![],
                        })),
                        None,
                    );
                } else {
                    sink.push_map_browse(
                        client_id,
                        request_id,
                        location.clone(),
                        None,
                        Some(format!("'{location}' has no scripted scene")),
                    );
                }
            }
            RemoteEvent::SettingsGet {
                client_id,
                request_id,
            } => {
                println!("EVENT settings_get");
                sink.push_settings(
                    client_id,
                    request_id,
                    settings_catalog.clone(),
                    None,
                    None,
                    false,
                );
            }
            RemoteEvent::SettingsPut {
                client_id,
                request_id,
                key,
                value,
                scope,
            } => {
                println!("EVENT settings_put: key={key:?} value={value} scope={scope:?}");
                let error = apply_scripted_setting(&mut settings_catalog, &key, &value);
                let saved = error.is_none();
                sink.push_settings(
                    client_id,
                    request_id,
                    serde_json::Value::Null,
                    Some(key),
                    error,
                    saved,
                );
            }
            RemoteEvent::HighlightsGet {
                client_id,
                request_id,
                scope,
            } => {
                println!("EVENT highlights_get: scope={scope:?}");
                let rules = serde_json::json!({
                    "bandit": { "pattern": "bandit", "fg": "#ff5050", "bold": true },
                    "treasure": { "pattern": "chest|coffer", "fg": "#d9b44f" },
                });
                sink.push_highlights(
                    client_id,
                    request_id,
                    scope,
                    rules,
                    vec!["alert.mp3".to_string()],
                    None,
                );
            }
            RemoteEvent::ColorsGet {
                client_id,
                request_id,
                scope,
            } => {
                println!("EVENT colors_get: scope={scope:?}");
                let colors = serde_json::json!({
                    "presets": {
                        "speech": { "fg": "#a0d0ff" },
                        "whisper": { "fg": "#80ff80", "bg": "#102010" },
                    },
                    "prompt_colors": [ { "character": ">", "fg": "#888888" } ],
                });
                sink.push_colors(client_id, request_id, scope, colors, None, false);
            }
            RemoteEvent::Command(text) => {
                println!("EVENT cmd: {text:?}");
                // `echo <text>` / `echot <text>` / `echod <text>` send the
                // text back as a main/thoughts/death stream line, so
                // client-side text-flow features (speech, highlights) can
                // be exercised end-to-end without a game connection.
                let mut echo = |stream: &str, body: &str| {
                    sink.push_text(
                        stream,
                        std::sync::Arc::new(StyledLine {
                            segments: vec![TextSegment::plain(body)],
                            stream: stream.to_string(),
                            timestamp: None,
                        }),
                    );
                };
                if let Some(body) = text.strip_prefix("echo ") {
                    echo("main", body);
                } else if let Some(body) = text.strip_prefix("echot ") {
                    echo("thoughts", body);
                } else if let Some(body) = text.strip_prefix("echod ") {
                    echo("death", body);
                }
            }
            RemoteEvent::Macro { id } => println!("EVENT macro tap: {id:?}"),
            RemoteEvent::MacroSave {
                group,
                label,
                command,
                color,
                confirm,
                insert,
                options,
                original,
            } => {
                println!(
                    "EVENT macro_save: label={label:?} command={command:?} insert={insert} options={options:?}"
                );
                let button = MacroButton {
                    label,
                    command: Some(command).filter(|c| !c.is_empty()),
                    color,
                    confirm,
                    insert,
                    options,
                    ..Default::default()
                };
                let original = original
                    .as_ref()
                    .map(|(g, l)| (g.as_deref(), l.as_str()));
                local.upsert_button(group.as_deref(), button, original);
                sink.set_macros(&MacrosConfig::merge(base.clone(), local.clone()));
            }
            RemoteEvent::MacroDelete { group, label } => {
                println!("EVENT macro_delete: {group:?} {label:?}");
                local.delete_button(group.as_deref(), &label);
                sink.set_macros(&MacrosConfig::merge(base.clone(), local.clone()));
            }
            other => println!("EVENT other: {other:?}"),
        }
    }
}

/// Validate one settings_put against the scripted catalog the way the
/// registry setters would (JSON type, int/float range, enum options) and
/// store the accepted value. Returns the rejection message, or None on
/// success. Sensitive rows keep `""` + redacted, like the real server.
fn apply_scripted_setting(
    catalog: &mut serde_json::Value,
    key: &str,
    value: &serde_json::Value,
) -> Option<String> {
    let Some(entry) = catalog
        .as_array_mut()
        .and_then(|a| a.iter_mut().find(|e| e["key"] == key))
    else {
        return Some(format!("Unknown setting '{key}'"));
    };
    let kind = entry["kind"].clone();
    let err = match kind["type"].as_str().unwrap_or("") {
        "bool" if !value.is_boolean() => Some("expected a boolean".to_string()),
        "int" => match value.as_i64() {
            None => Some("expected an integer".to_string()),
            Some(n) => {
                let (min, max) = (kind["min"].as_i64().unwrap(), kind["max"].as_i64().unwrap());
                (n < min || n > max).then(|| format!("must be between {min} and {max}"))
            }
        },
        "float" => match value.as_f64() {
            None => Some("expected a number".to_string()),
            Some(n) => {
                let (min, max) = (kind["min"].as_f64().unwrap(), kind["max"].as_f64().unwrap());
                (n < min || n > max).then(|| format!("must be between {min} and {max}"))
            }
        },
        "enum" => match value.as_str() {
            None => Some("expected text".to_string()),
            Some(s) => {
                let options = kind["options"].as_array().unwrap();
                (!options.iter().any(|o| o == s))
                    .then(|| format!("'{s}' is not one of the allowed options"))
            }
        },
        "text" | "optional_text" if !value.is_string() => Some("expected text".to_string()),
        "list" => {
            let ok = value
                .as_array()
                .is_some_and(|items| items.iter().all(serde_json::Value::is_string));
            (!ok).then(|| "expected a list of text entries".to_string())
        }
        _ => None,
    };
    if err.is_none() && entry["sensitive"] != true {
        entry["value"] = value.clone();
    } else if err.is_none() {
        entry["redacted"] = serde_json::json!(true);
    }
    err
}
