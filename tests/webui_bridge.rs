//! Integration tests for the Lich WebUI bridge client.
//!
//! Stands up a mock WebUI WebSocket server that enforces the same upgrade
//! requirements as lich-5's `lib/webui/server.rb` (auth cookie + loopback
//! Origin allowlist), then drives `vellum_fe::webui::start` against it:
//! hello -> subscribe -> render -> event round-trip, plus reconnect replay.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::Message;

use vellum_fe::data::webui::WebUiClientMessage;
use vellum_fe::webui::{self, WebUiEvent};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Accepts one WebSocket connection, validating the upgrade like Lich does:
/// auth cookie plus an Origin matching the dialed host:port (the server's
/// allowed-hosts check).
async fn accept_validated_from(
    listener: &tokio::net::TcpListener,
    host: &str,
    port: u16,
) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
    let (stream, _) = listener.accept().await.expect("accept");
    let expected_origin = format!("http://{}:{}", host, port);
    tokio_tungstenite::accept_hdr_async(stream, move |req: &Request, resp: Response| {
        let headers = req.headers();
        let cookie = headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            cookie.contains(&format!("lich_webui={}", TOKEN)),
            "upgrade must carry the auth cookie, got: {}",
            cookie
        );
        let origin = headers
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(
            origin, expected_origin,
            "upgrade Origin must match the dialed host:port"
        );
        Ok(resp)
    })
    .await
    .expect("websocket upgrade")
}

async fn accept_validated(
    listener: &tokio::net::TcpListener,
    port: u16,
) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
    accept_validated_from(listener, "127.0.0.1", port).await
}

fn hello_json() -> String {
    r#"{"type":"hello","schema_version":1,
        "session":{"name":"Testchar","game":"GSIV"},
        "pages":[{"id":"demo/demo","title":"Demo","script":"demo",
                  "kind":"panel","bare":true,"size":[320,90]}],
        "siblings":[]}"#
        .to_string()
}

fn render_json(seq: u64) -> String {
    format!(
        r#"{{"type":"render","page":"demo/demo","seq":{},
            "tree":{{"t":"page","title":"Demo","children":[
              {{"t":"text","cid":"text:0","text":"hello"}},
              {{"t":"button","cid":"button:1","label":"Go"}}]}}}}"#,
        seq
    )
}

async fn recv_event(rx: &mut mpsc::UnboundedReceiver<WebUiEvent>) -> WebUiEvent {
    tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for bridge event")
        .expect("bridge event channel closed")
}

#[tokio::test]
async fn bridge_full_round_trip() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let handle = webui::start(
        &tokio::runtime::Handle::current(),
        "127.0.0.1".into(),
        port,
        TOKEN.into(),
        event_tx,
    );

    let mut socket = accept_validated(&listener, port).await;
    socket.send(Message::text(hello_json())).await.unwrap();

    // Client surfaces the hello with session + page descriptors.
    let WebUiEvent::Hello { schema_version, session, pages } = recv_event(&mut event_rx).await
    else {
        panic!("expected Hello first");
    };
    assert_eq!(schema_version, 1);
    assert_eq!(session.name, "Testchar");
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].id, "demo/demo");
    assert_eq!(pages[0].size, Some([320.0, 90.0]));

    // Subscribe goes out as JSON.
    handle.subscribe("demo/demo");
    let raw = socket.next().await.unwrap().unwrap();
    assert_eq!(
        raw.to_text().unwrap(),
        r#"{"type":"subscribe","page":"demo/demo"}"#
    );

    // Render push comes back parsed.
    socket.send(Message::text(render_json(1))).await.unwrap();
    let WebUiEvent::Render { page, seq, tree } = recv_event(&mut event_rx).await else {
        panic!("expected Render");
    };
    assert_eq!(page, "demo/demo");
    assert_eq!(seq, 1);
    assert_eq!(tree.children().len(), 2);
    assert_eq!(tree.children()[1].label.as_deref(), Some("Go"));

    // Interaction events flow out.
    handle.send(WebUiClientMessage::Event {
        page: "demo/demo".into(),
        cid: "button:1".into(),
        value: serde_json::Value::Null,
    });
    let raw = socket.next().await.unwrap().unwrap();
    assert_eq!(
        raw.to_text().unwrap(),
        r#"{"type":"event","page":"demo/demo","cid":"button:1","value":null}"#
    );

    // Page close notice.
    socket
        .send(Message::text(r#"{"type":"close","page":"demo/demo"}"#))
        .await
        .unwrap();
    let WebUiEvent::PageClosed { page } = recv_event(&mut event_rx).await else {
        panic!("expected PageClosed");
    };
    assert_eq!(page, "demo/demo");
}

#[tokio::test]
async fn bridge_dials_the_handshake_host_not_loopback() {
    // A containerized Lich advertises a LAN address in the handshake url;
    // the bridge must dial that host and present it in the Origin. Dialing
    // "localhost" (resolves to the same listener, but a different Origin
    // string than 127.0.0.1) proves nothing is hardcoded to loopback.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let _handle = webui::start(
        &tokio::runtime::Handle::current(),
        "localhost".into(),
        port,
        TOKEN.into(),
        event_tx,
    );

    let mut socket = accept_validated_from(&listener, "localhost", port).await;
    socket.send(Message::text(hello_json())).await.unwrap();
    assert!(matches!(
        recv_event(&mut event_rx).await,
        WebUiEvent::Hello { .. }
    ));
}

#[tokio::test]
async fn fetch_image_sends_cookie_and_returns_body() {
    // Minimal 1x1 PNG (what the /files/ route would serve).
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
        0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
        0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Mock the Lich /files/ route: one request per connection, cookie
    // required, Connection: close (matching server.rb's respond()).
    let body = png.clone();
    let server = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0u8; 4096];
        let read = stream.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        assert!(request.starts_with("GET /files/cbcal/greyscale/hinterwilds/angargeist.png HTTP/1.1\r\n"));
        assert!(
            request.contains(&format!("Cookie: lich_webui={}", TOKEN)),
            "fetch must carry the auth cookie"
        );
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: private, max-age=60\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).await.unwrap();
        stream.write_all(&body).await.unwrap();
        stream.shutdown().await.unwrap();
        request
    });

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    webui::fetch_image(
        &tokio::runtime::Handle::current(),
        "127.0.0.1".into(),
        port,
        TOKEN.into(),
        "/files/cbcal/greyscale/hinterwilds/angargeist.png".into(),
        event_tx,
    );

    let WebUiEvent::ImageFetched { src, data } = recv_event(&mut event_rx).await else {
        panic!("expected ImageFetched");
    };
    assert_eq!(src, "/files/cbcal/greyscale/hinterwilds/angargeist.png");
    assert_eq!(data.expect("fetch should succeed"), png);
    server.await.unwrap();
}

#[tokio::test]
async fn fetch_image_reports_http_errors() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0u8; 4096];
        let _ = stream.read(&mut buffer).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
    });

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    webui::fetch_image(
        &tokio::runtime::Handle::current(),
        "127.0.0.1".into(),
        port,
        TOKEN.into(),
        "/files/cbcal/missing.png".into(),
        event_tx,
    );

    let WebUiEvent::ImageFetched { data, .. } = recv_event(&mut event_rx).await else {
        panic!("expected ImageFetched");
    };
    let err = data.expect_err("404 must surface as an error");
    assert!(err.contains("404"), "error should mention the status: {}", err);
}

#[tokio::test]
async fn bridge_reconnects_and_replays_subscriptions() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let handle = webui::start(
        &tokio::runtime::Handle::current(),
        "127.0.0.1".into(),
        port,
        TOKEN.into(),
        event_tx,
    );

    // First connection: hello, subscribe, then drop the socket.
    {
        let mut socket = accept_validated(&listener, port).await;
        socket.send(Message::text(hello_json())).await.unwrap();
        assert!(matches!(
            recv_event(&mut event_rx).await,
            WebUiEvent::Hello { .. }
        ));
        handle.subscribe("demo/demo");
        let raw = socket.next().await.unwrap().unwrap();
        assert!(raw.to_text().unwrap().contains("subscribe"));
        // server goes away (Lich script reload etc.)
        socket.close(None).await.unwrap();
    }

    assert!(matches!(
        recv_event(&mut event_rx).await,
        WebUiEvent::Disconnected { gave_up: false }
    ));

    // Second connection: the bridge reconnects on its own and replays the
    // subscription before anything else.
    let mut socket = accept_validated(&listener, port).await;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .expect("timed out waiting for replayed subscribe")
        .unwrap()
        .unwrap();
    assert_eq!(
        raw.to_text().unwrap(),
        r#"{"type":"subscribe","page":"demo/demo"}"#
    );
    socket.send(Message::text(hello_json())).await.unwrap();
    assert!(matches!(
        recv_event(&mut event_rx).await,
        WebUiEvent::Hello { .. }
    ));
    // Fresh render resumes the page.
    socket.send(Message::text(render_json(7))).await.unwrap();
    let WebUiEvent::Render { seq, .. } = recv_event(&mut event_rx).await else {
        panic!("expected Render after reconnect");
    };
    assert_eq!(seq, 7);
}
