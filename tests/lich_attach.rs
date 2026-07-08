//! Integration test for the Lich detachable-client attach path the mobile
//! apps use: a fake Lich accepts the connection, receives the detachable
//! handshake and bare (un-prefixed) commands, streams game lines back, and
//! its close ends the network task — which is exactly the completion the
//! headless supervisor keys re-attach off of.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use vellum_fe::network::{LichConnection, ServerMessage};

const WAIT: std::time::Duration = std::time::Duration::from_secs(5);

async fn recv(rx: &mut mpsc::Receiver<ServerMessage>) -> ServerMessage {
    tokio::time::timeout(WAIT, rx.recv())
        .await
        .expect("timed out waiting for server message")
        .expect("server channel closed")
}

#[tokio::test(flavor = "multi_thread")]
async fn detachable_attach_handshakes_streams_and_ends_on_close() {
    let fake = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake lich");
    let addr = fake.local_addr().unwrap();

    // Fake Lich: forward every inbound line for inspection, emit two game
    // lines on attach, close the socket when told to.
    let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel::<String>();
    let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let (stream, _) = fake.accept().await.expect("accept");
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        write
            .write_all(b"<pushBold/>Welcome back<popBold/>\nYou see nothing unusual.\n")
            .await
            .expect("write game lines");
        write.flush().await.expect("flush");
        let forward = async {
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = inbound_tx.send(line);
            }
        };
        tokio::select! {
            _ = forward => {}
            _ = close_rx => {}
        }
    });

    let (server_tx, mut server_rx) = mpsc::channel::<ServerMessage>(64);
    let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();
    let task = tokio::spawn(async move {
        LichConnection::start("127.0.0.1", addr.port(), None, server_tx, command_rx, None).await
    });

    // Session comes up and the fake's lines arrive as text.
    assert!(matches!(recv(&mut server_rx).await, ServerMessage::Connected));
    match recv(&mut server_rx).await {
        ServerMessage::Text(line) => assert_eq!(line, "<pushBold/>Welcome back<popBold/>"),
        other => panic!("expected text, got {other:?}"),
    }
    match recv(&mut server_rx).await {
        ServerMessage::Text(line) => assert_eq!(line, "You see nothing unusual."),
        other => panic!("expected text, got {other:?}"),
    }

    // Detachable handshake reaches Lich (PID + frontend identity).
    let first = tokio::time::timeout(WAIT, inbound_rx.recv())
        .await
        .expect("timed out")
        .expect("fake closed");
    assert!(
        first.starts_with("SET_FRONTEND_PID "),
        "expected PID line, got '{first}'"
    );
    let second = tokio::time::timeout(WAIT, inbound_rx.recv())
        .await
        .expect("timed out")
        .expect("fake closed");
    assert_eq!(second, ";eq $frontend=\"stormfront\"");

    // Commands arrive bare — Lich's detachable client thread prepends <c>
    // itself; a prefixed command would reach the game as <c><c>cmd.
    command_tx.send("look".to_string()).expect("send command");
    let cmd = tokio::time::timeout(WAIT, inbound_rx.recv())
        .await
        .expect("timed out")
        .expect("fake closed");
    assert_eq!(cmd, "look");

    // Lich closing the socket surfaces Disconnected and ends the task —
    // the supervisor's re-attach trigger.
    close_tx.send(()).expect("signal close");
    assert!(matches!(
        recv(&mut server_rx).await,
        ServerMessage::Disconnected
    ));
    tokio::time::timeout(WAIT, task)
        .await
        .expect("network task did not end after server close")
        .expect("task panicked")
        .expect("task returned error");
}
