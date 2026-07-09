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
use vellum_fe::core::remote::{RemoteEvent, RemoteSink};
use vellum_fe::data::widget::{StyledLine, TextSegment};
use vellum_fe::frontend::web::server;

const TOKEN: &str = "abc123";

#[tokio::main]
async fn main() {
    let (mut sink, handles, mut event_rx) = RemoteSink::new(500);

    // --login: act like the headless (mobile) runtime with no session, so
    // the login overlay (and its Remote tab shell plumbing) can be driven.
    if std::env::args().any(|a| a == "--login") {
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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8399")
        .await
        .expect("bind 8399");
    println!("harness: http://127.0.0.1:8399/play#token={TOKEN}");
    tokio::spawn(async move {
        let _ = server::serve_listener_with_token(listener, handles, TOKEN.to_string()).await;
    });

    while let Some(event) = event_rx.recv().await {
        match event {
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
