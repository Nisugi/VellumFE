//! End-to-end tests for the web frontend sidecar: real TCP sockets, real
//! HTTP, and a minimal hand-rolled WebSocket client (no extra dev-deps).
//!
//! Covers the Phase 1 read-only path from docs/mobile-web-frontend-plan.md:
//! core sink -> ring buffer/broadcast -> axum server -> WS client.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use vellum_fe::core::remote::RemoteSink;
use vellum_fe::core::GameState;
use vellum_fe::data::widget::{StyledLine, TextSegment};
use vellum_fe::frontend::web::server;

async fn start_server(sink_capacity: usize) -> (RemoteSink, std::net::SocketAddr) {
    let (sink, handles) = RemoteSink::new(sink_capacity);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = server::serve_listener(listener, handles).await;
    });
    (sink, addr)
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

/// Minimal WS client: handshake then read unmasked server text frames.
struct WsClient {
    stream: TcpStream,
}

impl WsClient {
    async fn connect(addr: std::net::SocketAddr) -> Self {
        let mut stream = TcpStream::connect(addr).await.expect("connect ws");
        let req = format!(
            "GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        );
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
}

async fn read_json_timeout(client: &mut WsClient) -> serde_json::Value {
    tokio::time::timeout(std::time::Duration::from_secs(5), client.read_json())
        .await
        .expect("timed out waiting for a WS frame")
}

#[tokio::test]
async fn health_and_static_assets_are_served() {
    let (_sink, addr) = start_server(100).await;

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
    let (mut sink, addr) = start_server(100).await;

    // Lines buffered before the client connects land in its snapshot.
    sink.push_text("main", styled("pre-connect line", "main"));

    let mut client = WsClient::connect(addr).await;

    let hello = read_json_timeout(&mut client).await;
    assert_eq!(hello["v"], 1);
    assert_eq!(hello["t"], "hello");

    let snapshot = read_json_timeout(&mut client).await;
    assert_eq!(snapshot["t"], "snapshot");
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
    let (mut sink, addr) = start_server(100).await;

    let mut a = WsClient::connect(addr).await;
    let mut b = WsClient::connect(addr).await;
    // Drain hello + snapshot on both.
    for client in [&mut a, &mut b] {
        assert_eq!(read_json_timeout(client).await["t"], "hello");
        assert_eq!(read_json_timeout(client).await["t"], "snapshot");
    }

    sink.push_text("main", styled("fan-out", "main"));

    for client in [&mut a, &mut b] {
        let delta = read_json_timeout(client).await;
        assert_eq!(delta["t"], "text");
        assert_eq!(delta["d"]["line"]["segments"][0]["text"], "fan-out");
    }
}
