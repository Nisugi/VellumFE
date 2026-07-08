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

/// Aborting the network task (disconnect button, stall watchdog) must
/// actually close the socket. The reader runs as its own spawned task
/// holding half of the split stream; if the abort merely detaches it, the
/// connection stays open, Lich's single detachable-client slot stays
/// occupied, and no re-attach succeeds until the process dies (the
/// force-close-to-reconnect bug from device testing).
#[tokio::test(flavor = "multi_thread")]
async fn aborting_the_task_closes_the_socket_so_reattach_works() {
    let fake = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake lich");
    let addr = fake.local_addr().unwrap();

    // Like Lich: one client at a time; signal when a client's socket
    // closes (read loop ends) before accepting the next.
    let (closed_tx, mut closed_rx) = mpsc::unbounded_channel::<u32>();
    tokio::spawn(async move {
        let mut client = 0u32;
        loop {
            let (stream, _) = fake.accept().await.expect("accept");
            client += 1;
            let (read, mut write) = stream.into_split();
            write.write_all(b"attached\n").await.expect("greet");
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(_)) = lines.next_line().await {}
            let _ = closed_tx.send(client);
        }
    });

    // First attach; hold the command channel open so the write loop can't
    // end on its own — teardown must come from the abort alone.
    let (server_tx, mut server_rx) = mpsc::channel::<ServerMessage>(64);
    let (_command_tx, command_rx) = mpsc::unbounded_channel::<String>();
    let task = tokio::spawn(async move {
        LichConnection::start("127.0.0.1", addr.port(), None, server_tx, command_rx, None).await
    });
    assert!(matches!(recv(&mut server_rx).await, ServerMessage::Connected));

    // Supervisor-style teardown.
    task.abort();
    let _ = task.await;

    // The fake must observe the close; a detached reader leaks the socket
    // and this times out.
    let closed = tokio::time::timeout(WAIT, closed_rx.recv())
        .await
        .expect("socket never closed after aborting the network task")
        .expect("fake exited");
    assert_eq!(closed, 1);

    // With the slot free, a second attach from the same process works.
    let (server_tx2, mut server_rx2) = mpsc::channel::<ServerMessage>(64);
    let (_command_tx2, command_rx2) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        LichConnection::start("127.0.0.1", addr.port(), None, server_tx2, command_rx2, None).await
    });
    assert!(matches!(recv(&mut server_rx2).await, ServerMessage::Connected));
    match recv(&mut server_rx2).await {
        ServerMessage::Text(line) => assert_eq!(line, "attached"),
        other => panic!("expected text, got {other:?}"),
    }
}
