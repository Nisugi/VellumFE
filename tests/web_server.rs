//! End-to-end tests for the web frontend sidecar: real TCP sockets, real
//! HTTP, and a minimal hand-rolled WebSocket client (no extra dev-deps).
//!
//! Covers the read-only path (Phase 1) and input/dual-control (Phase 2)
//! from docs/mobile-web-frontend-plan.md: core sink -> ring buffer /
//! broadcast -> axum server -> WS client, plus client cmd -> RemoteEvent
//! and reconnect-with-resume.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use vellum_fe::core::remote::{RemoteEvent, RemoteSink};
use vellum_fe::core::GameState;
use vellum_fe::data::widget::{StyledLine, TextSegment};
use vellum_fe::frontend::web::server;

async fn start_server(
    sink_capacity: usize,
) -> (
    RemoteSink,
    mpsc::UnboundedReceiver<RemoteEvent>,
    std::net::SocketAddr,
) {
    let (sink, handles, event_rx) = RemoteSink::new(sink_capacity);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = server::serve_listener(listener, handles).await;
    });
    (sink, event_rx, addr)
}

fn styled(text: &str, stream: &str) -> Arc<StyledLine> {
    Arc::new(StyledLine {
        segments: vec![TextSegment::plain(text)],
        stream: stream.to_string(),
    })
}

async fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf).into_owned()
}

/// Minimal WS client: handshake, read unmasked server text frames, send
/// masked client text frames (RFC 6455 requires client frames be masked).
struct WsClient {
    stream: TcpStream,
}

impl WsClient {
    async fn connect(addr: std::net::SocketAddr) -> Self {
        let mut stream = TcpStream::connect(addr).await.expect("connect ws");
        let req = "GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n";
        stream.write_all(req.as_bytes()).await.unwrap();

        // Read until the end of the HTTP response headers.
        let mut headers = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).await.expect("handshake read");
            headers.push(byte[0]);
            if headers.ends_with(b"\r\n\r\n") {
                break;
            }
            assert!(headers.len() < 8192, "handshake response too large");
        }
        let response = String::from_utf8_lossy(&headers).into_owned();
        assert!(
            response.starts_with("HTTP/1.1 101"),
            "expected 101 Switching Protocols, got:\n{response}"
        );
        Self { stream }
    }

    /// Read one text frame's payload as parsed JSON.
    async fn read_json(&mut self) -> serde_json::Value {
        let mut header = [0u8; 2];
        self.stream.read_exact(&mut header).await.expect("frame header");
        let opcode = header[0] & 0x0f;
        assert_eq!(opcode, 0x1, "expected a text frame");
        assert_eq!(header[0] & 0x80, 0x80, "expected FIN (no fragmentation)");
        assert_eq!(header[1] & 0x80, 0, "server frames must be unmasked");
        let len = match header[1] & 0x7f {
            126 => {
                let mut ext = [0u8; 2];
                self.stream.read_exact(&mut ext).await.unwrap();
                u16::from_be_bytes(ext) as usize
            }
            127 => {
                let mut ext = [0u8; 8];
                self.stream.read_exact(&mut ext).await.unwrap();
                u64::from_be_bytes(ext) as usize
            }
            n => n as usize,
        };
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await.expect("frame payload");
        serde_json::from_slice(&payload).expect("frame payload is JSON")
    }

    /// Send one masked text frame (7-bit and 16-bit lengths suffice here).
    async fn send_text(&mut self, payload: &str) {
        let bytes = payload.as_bytes();
        let mask = [0x12u8, 0x34, 0x56, 0x78];
        let mut frame = vec![0x81u8];
        if bytes.len() < 126 {
            frame.push(0x80 | bytes.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(
            bytes
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ mask[i % 4]),
        );
        self.stream.write_all(&frame).await.expect("send frame");
    }

    async fn send_resume(&mut self, seq: u64) {
        self.send_text(&format!(r#"{{"t":"resume","d":{{"seq":{seq}}}}}"#))
            .await;
    }
}

async fn read_json_timeout(client: &mut WsClient) -> serde_json::Value {
    tokio::time::timeout(std::time::Duration::from_secs(5), client.read_json())
        .await
        .expect("timed out waiting for a WS frame")
}

/// Connect, drain hello (answering with resume seq), return the client and
/// the snapshot message.
async fn connect_and_sync(addr: std::net::SocketAddr, resume_seq: u64) -> (WsClient, serde_json::Value) {
    let mut client = WsClient::connect(addr).await;
    let hello = read_json_timeout(&mut client).await;
    assert_eq!(hello["t"], "hello");
    client.send_resume(resume_seq).await;
    let snapshot = read_json_timeout(&mut client).await;
    assert_eq!(snapshot["t"], "snapshot");
    (client, snapshot)
}

#[tokio::test]
async fn health_and_static_assets_are_served() {
    let (_sink, _event_rx, addr) = start_server(100).await;

    let health = http_get(addr, "/health").await;
    assert!(health.contains("200"), "health: {health}");
    assert!(health.ends_with("ok"), "health body: {health}");

    let index = http_get(addr, "/").await;
    assert!(index.contains("200"));
    assert!(index.contains("VellumFE"));

    let js = http_get(addr, "/app.js").await;
    assert!(js.contains("text/javascript"));

    let css = http_get(addr, "/app.css").await;
    assert!(css.contains("text/css"));
}

#[tokio::test]
async fn ws_client_gets_hello_snapshot_then_live_deltas() {
    let (mut sink, _event_rx, addr) = start_server(100).await;

    // Lines buffered before the client connects land in its snapshot.
    sink.push_text("main", styled("pre-connect line", "main"));

    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    assert_eq!(snapshot["d"]["mode"], "full");
    let text = snapshot["d"]["text"].as_array().unwrap();
    assert_eq!(text.len(), 1);
    assert_eq!(text[0]["stream"], "main");
    assert_eq!(text[0]["line"]["segments"][0]["text"], "pre-connect line");

    // A line pushed after connect arrives as a live text delta.
    sink.push_text("main", styled("live line", "main"));
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "text");
    assert_eq!(delta["seq"], 2);
    assert_eq!(delta["d"]["line"]["segments"][0]["text"], "live line");

    // State changes flow as coalesced deltas.
    let mut gs = GameState::new();
    gs.vitals.health = 42;
    sink.flush_state(&gs);
    let vitals = read_json_timeout(&mut client).await;
    assert_eq!(vitals["t"], "vitals");
    assert_eq!(vitals["d"]["health"], 42);
}

#[tokio::test]
async fn two_clients_both_receive_broadcasts() {
    let (mut sink, _event_rx, addr) = start_server(100).await;

    let (mut a, _) = connect_and_sync(addr, 0).await;
    let (mut b, _) = connect_and_sync(addr, 0).await;

    sink.push_text("main", styled("fan-out", "main"));

    for client in [&mut a, &mut b] {
        let delta = read_json_timeout(client).await;
        assert_eq!(delta["t"], "text");
        assert_eq!(delta["d"]["line"]["segments"][0]["text"], "fan-out");
    }
}

#[tokio::test]
async fn client_cmd_arrives_as_remote_event() {
    let (_sink, mut event_rx, addr) = start_server(100).await;

    let (mut client, _) = connect_and_sync(addr, 0).await;
    client
        .send_text(r#"{"t":"cmd","d":{"text":"look"}}"#)
        .await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out waiting for remote event")
        .expect("event channel open");
    let RemoteEvent::Command(text) = event else { panic!("expected Command event") };
    assert_eq!(text, "look");

    // Unknown/malformed messages are ignored, not fatal.
    client.send_text(r#"{"t":"bogus","d":{}}"#).await;
    client.send_text("not json").await;
    client
        .send_text(r#"{"t":"cmd","d":{"text":"second"}}"#)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::Command(text) = event else { panic!("expected Command event") };
    assert_eq!(text, "second");
}

#[tokio::test]
async fn link_tap_becomes_remote_event_and_menu_routes_to_requester_only() {
    let (mut sink, mut event_rx, addr) = start_server(100).await;

    let (mut tapper, _) = connect_and_sync(addr, 0).await;
    let (mut other, _) = connect_and_sync(addr, 0).await;

    tapper
        .send_text(r#"{"t":"link_tap","d":{"request_id":7,"exist_id":"12345","noun":"kobold","text":"a kobold","coord":null}}"#)
        .await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out waiting for link tap")
        .expect("event channel open");
    let RemoteEvent::LinkTap {
        client_id,
        request_id,
        exist_id,
        noun,
        text,
        coord,
    } = event
    else {
        panic!("expected LinkTap event");
    };
    assert_eq!(request_id, 7);
    assert_eq!(exist_id, "12345");
    assert_eq!(noun, "kobold");
    assert_eq!(text, "a kobold");
    assert_eq!(coord, None);

    // A coord link (e.g. an exit) carries its coord through so the main
    // loop can resolve the default command instead of raising a menu.
    tapper
        .send_text(r#"{"t":"link_tap","d":{"request_id":8,"exist_id":"-10966483","noun":"south","text":"south","coord":"2524,1864"}}"#)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::LinkTap { coord, .. } = event else {
        panic!("expected LinkTap event");
    };
    assert_eq!(coord.as_deref(), Some("2524,1864"));

    // Simulate the core answering the tagged menu request.
    sink.push_menu(
        client_id,
        7,
        "kobold".to_string(),
        vec![vellum_fe::core::remote::RemoteMenuItem {
            text: "attack kobold".to_string(),
            command: "attack #12345".to_string(),
            disabled: false,
        }],
    );
    // Follow with a broadcast line so the non-requesting client has
    // something to receive if (and only if) the menu was filtered out.
    sink.push_text("main", styled("after-menu", "main"));

    let menu = read_json_timeout(&mut tapper).await;
    assert_eq!(menu["t"], "menu", "requester gets the menu first");
    assert_eq!(menu["d"]["request_id"], 7);
    assert_eq!(menu["d"]["noun"], "kobold");
    assert_eq!(menu["d"]["items"][0]["command"], "attack #12345");
    assert!(menu["d"]["items"][0].get("client_id").is_none());

    let next_for_other = read_json_timeout(&mut other).await;
    assert_eq!(
        next_for_other["t"], "text",
        "non-requesting client must skip the menu and see only the text"
    );
    assert_eq!(
        next_for_other["d"]["line"]["segments"][0]["text"],
        "after-menu"
    );
}

#[tokio::test]
async fn resume_replays_only_missed_lines() {
    let (mut sink, _event_rx, addr) = start_server(100).await;

    sink.push_text("main", styled("one", "main")); // seq 1
    sink.push_text("main", styled("two", "main")); // seq 2

    // First client saw everything up to seq 1, then "disconnected".
    let (_stale, _) = connect_and_sync(addr, 0).await;

    sink.push_text("main", styled("three", "main")); // seq 3

    // Reconnect with cursor at 1: replay must contain exactly 2 and 3.
    let (_client, snapshot) = connect_and_sync(addr, 1).await;
    assert_eq!(snapshot["d"]["mode"], "resume");
    let text = snapshot["d"]["text"].as_array().unwrap();
    let seqs: Vec<u64> = text.iter().map(|l| l["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs, vec![2, 3]);
}

#[tokio::test]
async fn resume_with_evicted_gap_falls_back_to_gap_snapshot() {
    // Tiny ring: 2 lines per stream.
    let (mut sink, _event_rx, addr) = start_server(2).await;

    for i in 1..=5 {
        sink.push_text("main", styled(&format!("line {i}"), "main"));
    }
    // Client last saw seq 1; seqs 2-3 have been evicted.
    let (_client, snapshot) = connect_and_sync(addr, 1).await;
    assert_eq!(snapshot["d"]["mode"], "gap");
    let text = snapshot["d"]["text"].as_array().unwrap();
    let seqs: Vec<u64> = text.iter().map(|l| l["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs, vec![4, 5], "gap snapshot carries the retained tail");
}
