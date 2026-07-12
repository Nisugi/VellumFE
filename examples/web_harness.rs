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
        snap.map_scene = RemoteMapSceneRef(Some(scene));
        snap.map_state = RemoteMapState {
            available: true,
            location: Some("Harness Town".into()),
            room: Some(102),
            cell: Some([2, 0]),
            in_ghost: false,
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
            RemoteEvent::Command(text) => println!("EVENT cmd: {text:?}"),
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
