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

/// The scripted room's portal commands (dynamic "portals" wheel).
fn scripted_portals() -> Vec<String> {
    vec!["go gate".into(), "climb ladder".into()]
}

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

    // Scripted Lich WebUI: advertise availability + a registered page, and
    // mark the bridge connected, so the phone's WebUI panel + picker work
    // without a real Lich. The render tree is served on subscribe below.
    sink.set_webui_available(true);
    sink.push_webui_pages(vec![vellum_fe::data::webui::WebUiPageDescriptor {
        id: "demo/panel".into(),
        title: "Demo Panel".into(),
        script: "demo".into(),
        kind: Some("panel".into()),
        bare: true,
        size: None,
    }]);
    sink.push_webui_connected(true);

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

    // Scripted radial wheels for the controller wheel (hold a wheel-bound
    // pad button): a default ring with a folder slice plus a named wheel.
    // wheel_pick resolves against this config exactly like the runtimes.
    let wheel_config = {
        use vellum_fe::config::WheelSlice;
        let slice = |label: &str, command: &str, color: Option<&str>| WheelSlice {
            label: label.into(),
            command: command.into(),
            color: color.map(str::to_owned),
            ..Default::default()
        };
        let mut config = vellum_fe::config::Config::default();
        config.controller_wheel = vec![
            slice("look", "look", None),
            slice("search", "search", None),
            slice("stand", "stand", None),
            WheelSlice {
                label: "stance".into(),
                command: String::new(),
                slices: vec![
                    slice("defensive", "stance defensive", Some("#2e8b57")),
                    slice("neutral", "stance neutral", Some("#4682b4")),
                    slice("offensive", "stance offensive", Some("#b03030")),
                ],
                // Give the folder an explicit span so the payload exercises
                // the variable-width fields end-to-end (harmless until B2).
                span: Some(120.0),
                ..Default::default()
            },
            slice("hide", "hide", None),
            slice("portal", ".portal", Some("#d9b44f")),
        ];
        config.controller_wheels.insert(
            "spells".into(),
            vec![
                slice("prep 101", "prep 101", None),
                slice("cast", "cast", None),
            ],
        );
        // Give the spells wheel a per-wheel aim-stick override so the
        // wheel_stick payload is exercised end-to-end.
        config.controller_wheels_meta.insert(
            "spells".into(),
            vellum_fe::config::WheelMeta {
                button: Some("l3".into()),
                stick: Some("right".into()),
                // Exercises the per-wheel start payload (inert until B2).
                start: Some(-30.0),
            },
        );
        // Non-default fire mode so the tuning payload carries fire_mode /
        // edge_threshold / retract_delta for the client to parse.
        config.controller_tuning.fire_mode = "retract".into();
        config.controller_tuning.retract_delta = 12;
        config
    };
    sink.set_wheels(&wheel_config);

    sink.push_text(
        "main",
        std::sync::Arc::new(StyledLine {
            segments: vec![TextSegment::plain(
                "[web_harness] ready — no game connected]",
            )],
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
        rooms.push(RemoteMapRoom {
            i: 300,
            x: 7,
            y: 0,
            e: false,
        });
        edges.push(edge(4, 0, 7, 0, 1, Some("go gate"), None, None));
        rooms.push(RemoteMapRoom {
            i: 400,
            x: 0,
            y: 6,
            e: false,
        });
        edges.push(edge(0, 0, 0, 6, 2, None, Some(100), Some(400)));
        let scene = std::sync::Arc::new(RemoteMapScene {
            location: "Harness Town".into(),
            sheet: "outdoor".into(),
            rooms,
            edges,
            labels: vec![RemoteMapLabel {
                x: 2,
                y: 1,
                t: "The Grid".into(),
            }],
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
        // Scripted room entities so interact mode has something to cycle:
        // two creatures (one with statuses), loot, a player. Exits ride
        // the room payload; the harness has no room, so exits stay empty
        // unless a snapshot sets them.
        {
            use vellum_fe::core::remote::RemoteRoomEntity;
            let entity = |label: &str, id: &str, noun: &str| RemoteRoomEntity {
                id: id.into(),
                label: label.into(),
                noun: noun.into(),
            };
            snap.entities.creatures = vec![
                entity("a muddy hog (stunned)", "111", "hog"),
                entity("a kobold", "222", "kobold"),
            ];
            snap.entities.objects = vec![entity("a silver ring", "333", "ring")];
            snap.entities.players = vec![entity("Testy", "444", "Testy")];
            snap.exits = vec!["n".into(), "sw".into(), "out".into()];
            // Portal commands for the dynamic "portals" wheel (also used
            // by the wheel_pick resolver below, like AppCore's).
            snap.portals = scripted_portals();
        }
        // Creature-field cards for /creatures, placed by the REAL solver so
        // the harness exercises actual placement, not hand-picked rects.
        {
            use vellum_fe::core::creature_cards::solver::{CardSize, CreatureField};
            use vellum_fe::core::remote::RemoteFieldCard;
            let mut cf = CreatureField::default();
            cf.arrive("111", CardSize::default(), CardSize::default());
            let boss = CardSize::new(0.78, 1.52);
            cf.arrive("222", boss, CardSize::new(1.37, 0.53)); // boss-sized
            let spider = CardSize::new(0.92, 0.60);
            cf.arrive("555", spider, CardSize::new(0.92, 0.42)); // low, wide
            let meta = |id: &str| match id {
                "111" => (
                    "hog",
                    "a muddy hog",
                    vec!["stunned".to_string()],
                    false,
                    false,
                ),
                "222" => ("kobold", "a kobold", vec![], true, true), // boss + current
                _ => (
                    "spider",
                    "a dusky spider",
                    vec!["webbed".to_string()],
                    false,
                    false,
                ),
            };
            snap.field = cf
                .draw_order()
                .iter()
                .map(|&i| {
                    let u = &cf.units()[i];
                    let r = cf.rect(u);
                    let (fx, fy) = cf.foot(u);
                    let (noun, name, statuses, boss, current) = meta(&u.members[0]);
                    RemoteFieldCard {
                        id: u.members[0].clone(),
                        noun: noun.into(),
                        name: name.into(),
                        rect: [r.x0, r.y0, r.x1, r.y1],
                        foot: [fx, fy],
                        dead: false,
                        boss,
                        current,
                        statuses,
                        lift: None,
                    }
                })
                .collect();
        }
        // Scripted room name + description prose so the status drawer's Room
        // section has a "look" to render, and a spellbook so the Spells
        // section is populated (both P2 feeds). Both ride the wire as styled
        // lines; the room prose carries a clickable scenery link so the
        // phone's tap-to-interact path can be exercised.
        {
            use vellum_fe::data::widget::{LinkData, StyledLine, TextSegment};
            let seg = |text: &str| TextSegment {
                text: text.into(),
                ..Default::default()
            };
            let link_seg = |text: &str, exist: &str, noun: &str| TextSegment {
                text: text.into(),
                link_data: Some(LinkData {
                    exist_id: exist.into(),
                    noun: noun.into(),
                    text: text.into(),
                    coord: None,
                }),
                ..Default::default()
            };
            let line = |segments: Vec<TextSegment>| StyledLine {
                segments,
                stream: "room".into(),
                timestamp: None,
            };
            snap.room_name = Some("Harness Town, Town Square".into());
            snap.room_description = vec![line(vec![
                seg("A cobbled square opens beneath a bright sky. A "),
                link_seg("marble fountain", "555", "fountain"),
                seg(" bubbles at its center, and shops line the surrounding streets."),
            ])];
            let spell = |text: &str| StyledLine {
                segments: vec![seg(text)],
                stream: "Spells".into(),
                timestamp: None,
            };
            snap.spellbook = vec![
                spell("Elemental Defense III (503)   00:14:59"),
                spell("Mana Leech (516)   00:29:42"),
                spell("Haste (506)   00:05:11"),
            ];
        }
        snap.map_scene = RemoteMapSceneRef(Some(scene));
        snap.map_state = RemoteMapState {
            available: true,
            location: Some("Harness Town".into()),
            room: Some(102),
            cell: Some([2, 0]),
            classic: None,
            in_ghost: false,
            // Scripted trip in progress: exercises the banner + Stop button.
            travel: Some(vellum_fe::core::remote::RemoteTravelStatus {
                dest: 300,
                done: 12,
                total: 47,
                eta: "1:04".into(),
            }),
            ghosts: vec![RemoteGhostNode {
                x: 3,
                y: -1,
                cur: false,
            }],
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
        let _ = server::serve_listener(
            listener,
            handles,
            TOKEN.to_string(),
            server::ServeOptions::default(),
        )
        .await;
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

    // Scripted streams catalog for the phone Streams panel: a subscribed
    // row (read-only on the phone), routed rows of each kind, and orphan
    // rows (one with a Lich friendly label), same wire shape as
    // streams_catalog_json. Puts update route + destination so a re-open
    // shows the edit; subscribed rows keep their destination (subscription
    // wins — the route is only the orphan policy).
    let mut streams_catalog = serde_json::json!({
        "streams": [
            { "id": "atmospherics", "label": "Ambient scenery", "destination": "fallback (main)",
              "subscribed": false, "route": null },
            { "id": "bounty", "label": null, "destination": "bounty_win",
              "subscribed": false, "route": "window:bounty_win" },
            { "id": "logons", "label": "Friends list", "destination": "thoughts +1",
              "subscribed": true, "route": "main" },
            { "id": "ooc", "label": null, "destination": "Main",
              "subscribed": false, "route": "main" },
            { "id": "speech", "label": null, "destination": "Discard",
              "subscribed": false, "route": "discard" },
            { "id": "thoughts", "label": null, "destination": "thoughts",
              "subscribed": true, "route": null },
        ],
        "windows": ["bounty_win", "main", "thoughts"],
        "fallback": "main",
    });

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
                clear,
            } => {
                println!(
                    "EVENT settings_put: key={key:?} value={value} scope={scope:?} clear={clear}"
                );
                let error = if clear {
                    clear_scripted_setting(&mut settings_catalog, &key)
                } else {
                    apply_scripted_setting(&mut settings_catalog, &key, &value)
                };
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
            RemoteEvent::StreamsGet {
                client_id,
                request_id,
            } => {
                println!("EVENT streams_get");
                sink.push_streams(
                    client_id,
                    request_id,
                    streams_catalog.clone(),
                    None,
                    None,
                    false,
                );
            }
            RemoteEvent::StreamsPut {
                client_id,
                request_id,
                stream,
                target,
            } => {
                println!("EVENT streams_put: stream={stream:?} target={target:?}");
                let error = apply_scripted_route(&mut streams_catalog, &stream, &target);
                let saved = error.is_none();
                sink.push_streams(
                    client_id,
                    request_id,
                    serde_json::Value::Null,
                    Some(stream),
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
                    "ui": {
                        "command_echo_color": "#cccccc",
                        "border_color": "#444444",
                        "focused_border_color": "#88aaff",
                        "text_color": "#e0e0e0",
                        "background_color": "#101010",
                        "selection_bg_color": "#334466",
                        "textarea_background": "#181818",
                    },
                    "spell_colors": [
                        { "spells": [101, 107], "color": "", "bar_color": "#00ffff", "text_color": "#ffffff", "bg_color": "#000000" },
                    ],
                    "color_palette": [
                        { "name": "danger", "color": "#ff3030", "category": "red", "favorite": false },
                    ],
                });
                sink.push_colors(client_id, request_id, scope, colors, None, false);
            }
            RemoteEvent::ColorsPut {
                client_id,
                request_id,
                scope,
                colors,
            } => {
                // Echo which sections the phone sent so a save round-trip is
                // visible on stdout (the highlight/colors editors send the
                // whole ColorConfig back).
                let sections: Vec<&str> = [
                    "presets",
                    "prompt_colors",
                    "ui",
                    "spell_colors",
                    "color_palette",
                ]
                .into_iter()
                .filter(|k| colors.get(*k).is_some())
                .collect();
                println!("EVENT colors_put: scope={scope:?} sections={sections:?} body={colors}");
                sink.push_colors(
                    client_id,
                    request_id,
                    scope,
                    serde_json::Value::Null,
                    None,
                    true,
                );
            }
            RemoteEvent::TouchWheelGet {
                client_id,
                request_id,
                scope,
            } => {
                println!("EVENT touch_wheel_get: scope={scope:?}");
                // Reply with a small scripted ring + the real catalog so the
                // editor renders its action picker from the true vocabulary.
                let slices = serde_json::json!([
                    { "label": "Room", "client": "open:room" },
                    { "label": "Look", "command": "look" },
                ]);
                let catalog = vellum_fe::config::touch_wheel_action_catalog();
                sink.push_touch_wheel(client_id, request_id, scope, slices, catalog, None, false);
            }
            RemoteEvent::TouchWheelPut {
                client_id,
                request_id,
                scope,
                slices,
            } => {
                println!("EVENT touch_wheel_put: scope={scope:?} slices={slices}");
                sink.push_touch_wheel(
                    client_id,
                    request_id,
                    scope,
                    serde_json::Value::Null,
                    serde_json::Value::Null,
                    None,
                    true,
                );
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
            RemoteEvent::LinkTap {
                client_id,
                request_id,
                exist_id,
                noun,
                ..
            } => {
                use vellum_fe::core::remote::RemoteMenuItem;
                println!("EVENT link_tap: exist_id={exist_id:?} noun={noun:?}");
                let item = |text: &str, command: &str| RemoteMenuItem {
                    text: text.into(),
                    command: command.into(),
                    disabled: false,
                };
                sink.push_menu(
                    client_id,
                    request_id,
                    noun.clone(),
                    vec![
                        item(&format!("look at {noun}"), &format!("look #{exist_id}")),
                        item(&format!("attack {noun}"), &format!("attack #{exist_id}")),
                    ],
                );
            }
            RemoteEvent::WheelPick { key, path } => {
                // Mirror AppCore::wheel_pick_command: the dynamic portals
                // wheel resolves by index against the live portal list.
                let resolved = if key == "portals" {
                    path.first()
                        .filter(|_| path.len() == 1)
                        .and_then(|&i| scripted_portals().into_iter().nth(i))
                } else {
                    wheel_config.wheel_pick_command(&key, &path)
                };
                println!("EVENT wheel_pick: key={key:?} path={path:?} resolved={resolved:?}");
            }
            RemoteEvent::MacroSave {
                group,
                label,
                command,
                color,
                confirm,
                insert,
                client,
                options,
                original,
            } => {
                println!(
                    "EVENT macro_save: label={label:?} command={command:?} insert={insert} client={client:?} options={options:?}"
                );
                let button = MacroButton {
                    label,
                    // A client-action button carries no game command.
                    command: Some(command)
                        .filter(|c| !c.is_empty())
                        .filter(|_| client.is_none()),
                    client,
                    color,
                    confirm,
                    insert,
                    options,
                    ..Default::default()
                };
                let original = original.as_ref().map(|(g, l)| (g.as_deref(), l.as_str()));
                local.upsert_button(group.as_deref(), button, original);
                sink.set_macros(&MacrosConfig::merge(base.clone(), local.clone()));
            }
            RemoteEvent::MacroDelete { group, label } => {
                println!("EVENT macro_delete: {group:?} {label:?}");
                local.delete_button(group.as_deref(), &label);
                sink.set_macros(&MacrosConfig::merge(base.clone(), local.clone()));
            }
            RemoteEvent::WebUiSubscribe { page } => {
                println!("EVENT webui_subscribe: page={page:?}");
                // Reply with a render tree exercising many node types so the
                // phone renderer can be verified end to end.
                let tree: vellum_fe::data::webui::WebUiNode = serde_json::from_value(serde_json::json!({
                    "t": "page", "title": "Demo Panel", "bare": true, "children": [
                        { "t": "header", "cid": "h0", "text": "Creatures" },
                        { "t": "text", "cid": "t0", "text": "A kobold lurks nearby." },
                        { "t": "markdown", "cid": "m0", "text": "Status: {{green:healthy}} — **ready** to *fight*." },
                        { "t": "divider" },
                        { "t": "button", "cid": "attack", "label": "Attack", "variant": "danger" },
                        { "t": "button", "cid": "flee", "label": "Flee", "confirm": "Confirm flee?" },
                        { "t": "text_input", "cid": "note", "label": "Note", "value": "hi", "placeholder": "type…" },
                        { "t": "checkbox", "cid": "auto", "label": "Auto-attack", "checked": true },
                        { "t": "select", "cid": "stance", "label": "Stance", "options": ["offensive","defensive"], "value": "defensive" },
                        { "t": "slider", "cid": "vol", "label": "Volume", "min": 0.0, "max": 100.0, "value": 40.0 },
                        { "t": "progress", "cid": "hp", "value": 0.6, "label": "HP 60%" },
                        { "t": "table", "cid": "tbl", "headings": ["Name","HP"], "rows": [["kobold","12"],["orc","40"]], "clickable": true, "selected": 1 },
                        { "t": "expander", "cid": "adv", "label": "Advanced", "open": false, "children": [
                            { "t": "text", "cid": "adv0", "text": "hidden detail" } ] },
                        { "t": "tabs", "cid": "tabs", "children": [
                            { "t": "tab", "cid": "ta", "label": "One", "children": [ { "t": "text", "cid": "x", "text": "tab one" } ] },
                            { "t": "tab", "cid": "tb", "label": "Two", "children": [ { "t": "text", "cid": "y", "text": "tab two" } ] } ] }
                    ]
                })).unwrap();
                sink.push_webui_render(page, 1, tree);
            }
            RemoteEvent::WebUiEvent { page, cid, value } => {
                println!("EVENT webui_event: page={page:?} cid={cid:?} value={value}");
            }
            other => println!("EVENT other: {other:?}"),
        }
    }
}

/// Clear one sensitive setting the way the real server does: only
/// sensitive rows may be cleared; the stored value resets to unset
/// (value "" stays, the redacted flag drops).
fn clear_scripted_setting(catalog: &mut serde_json::Value, key: &str) -> Option<String> {
    let Some(entry) = catalog
        .as_array_mut()
        .and_then(|a| a.iter_mut().find(|e| e["key"] == key))
    else {
        return Some(format!("Unknown setting '{key}'"));
    };
    if entry["sensitive"] != true {
        return Some(format!("Setting '{key}' cannot be cleared"));
    }
    entry["value"] = serde_json::json!("");
    if let Some(obj) = entry.as_object_mut() {
        obj.remove("redacted");
    }
    None
}

/// Apply one streams_put to the scripted streams catalog the way the real
/// handler does: validate the target, store it as the row's route, and
/// recompute the display destination (subscribed rows keep theirs — the
/// route is only the orphan policy). Unknown streams get a fresh orphan
/// row, like routing a stream the layout has never seen.
fn apply_scripted_route(
    catalog: &mut serde_json::Value,
    stream: &str,
    target: &str,
) -> Option<String> {
    let (route, orphan_destination): (serde_json::Value, String) = match target {
        "discard" => ("discard".into(), "Discard".to_string()),
        "main" => ("main".into(), "Main".to_string()),
        "clear" => (serde_json::Value::Null, "fallback (main)".to_string()),
        other => match other.strip_prefix("window:") {
            Some("") | None => {
                return Some(format!("invalid stream route {target:?}"));
            }
            Some(name) => (other.into(), name.to_string()),
        },
    };
    let rows = catalog["streams"].as_array_mut().expect("streams array");
    let row = match rows.iter_mut().find(|r| r["id"] == stream) {
        Some(row) => row,
        None => {
            rows.push(serde_json::json!({
                "id": stream, "label": null, "destination": "",
                "subscribed": false, "route": null,
            }));
            rows.last_mut().expect("just pushed")
        }
    };
    row["route"] = route;
    if row["subscribed"] != true {
        row["destination"] = serde_json::json!(orphan_destination);
    }
    None
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
