//! End-to-end tests for the web frontend sidecar: real TCP sockets, real
//! HTTP, and a minimal hand-rolled WebSocket client (no extra dev-deps).
//!
//! Covers the read-only path (Phase 1) and input/dual-control (Phase 2)
//! from docs/mobile-web-frontend-plan.md: core sink -> ring buffer /
//! broadcast -> axum server -> WS client, plus client cmd -> RemoteEvent
//! and reconnect-with-resume.

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use vellum_fe::config::WebConfig;
use vellum_fe::core::classic_maps::ClassicMapCatalog;
use vellum_fe::core::remote::{RemoteEvent, RemoteSessionInfo, RemoteSink, SessionState};
use vellum_fe::core::GameState;
use vellum_fe::data::widget::{StyledLine, TextSegment};
use vellum_fe::frontend::web::server;

const TEST_TOKEN: &str = "test-token";
const FIXTURE_HOST_ENV: &str = "DESPANA_PROCESS_FIXTURE";
const FIXTURE_PORT_ENV: &str = "DESPANA_PROCESS_FIXTURE_PORT";
const WALKED_PORT_FIXTURE_ENV: &str = "DESPANA_WALKED_PORT_FIXTURE";
const TOKEN_FAILURE_FIXTURE_ENV: &str = "DESPANA_TOKEN_FAILURE_FIXTURE";
const REGISTRY_FAILURE_FIXTURE_ENV: &str = "DESPANA_REGISTRY_FAILURE_FIXTURE";

async fn start_server(
    sink_capacity: usize,
) -> (
    RemoteSink,
    mpsc::UnboundedReceiver<RemoteEvent>,
    std::net::SocketAddr,
) {
    start_server_with_catalog(sink_capacity, Arc::new(ClassicMapCatalog::new())).await
}

async fn start_server_with_catalog(
    sink_capacity: usize,
    classic_maps: Arc<ClassicMapCatalog>,
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
        let _ = server::serve_listener(
            listener,
            handles,
            TEST_TOKEN.to_string(),
            server::ServeOptions {
                status_only: false,
                classic_maps,
            },
        )
        .await;
    });
    (sink, event_rx, addr)
}

/// Hidden child-process entry point for the process-isolation test below.
///
/// The ordinary integration-test run skips this function. The parent starts
/// this same test executable with `--ignored --exact` and the two fixture
/// environment variables set, giving each child its own runtime, RemoteSink,
/// TCP listener, and WebSocket session id without adding a production test
/// hook or a second server implementation.
#[tokio::test]
#[ignore]
async fn process_isolation_fixture_host() {
    let Ok(marker) = std::env::var(FIXTURE_HOST_ENV) else {
        return;
    };
    let port: u16 = std::env::var(FIXTURE_PORT_ENV)
        .expect("fixture port")
        .parse()
        .expect("numeric fixture port");

    let (mut sink, handles, mut event_rx) = RemoteSink::new(100);
    let mut state = GameState::new();
    state.character_name = Some(marker.clone());
    state.room_id = Some(format!("room-{marker}"));
    state.room_name = Some(format!("{marker} Room"));
    sink.push_text("main", styled(&format!("{marker} story marker"), "main"));
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&state, &[]));

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .expect("bind fixture server");
    tokio::spawn(async move {
        let _ = server::serve_listener(
            listener,
            handles,
            TEST_TOKEN.to_string(),
            server::ServeOptions::default(),
        )
        .await;
    });

    // Model the real frontend/core event pump: a command received by this
    // process is acknowledged back into this process's own story stream.
    while let Some(event) = event_rx.recv().await {
        if let RemoteEvent::Command { text: command, .. } = event {
            sink.push_text("main", styled(&format!("{marker} ack: {command}"), "main"));
        }
    }
}

/// Hidden child-process entry point for the configured-server readiness test.
///
/// `server::serve` intentionally owns token loading and session-registry
/// publication, so run it in a fresh process whose `VELLUM_FE_DIR` points at a
/// temporary directory. Besides avoiding the user's config, this ensures the
/// registry's process-wide cached directory cannot have been initialized by a
/// different test first.
#[tokio::test]
#[ignore]
async fn walked_port_readiness_fixture_host() {
    if std::env::var_os(WALKED_PORT_FIXTURE_ENV).is_none() {
        return;
    }
    let base_port: u16 = std::env::var(FIXTURE_PORT_ENV)
        .expect("fixture port")
        .parse()
        .expect("numeric fixture port");

    let (sink, handles, mut event_rx) = RemoteSink::new(100);
    let mut launch_endpoint_rx = sink.launch_endpoint_receiver();
    let config = WebConfig {
        enabled: true,
        multiaccount: false,
        port: base_port,
        bind: std::net::Ipv4Addr::LOCALHOST.to_string(),
        pinned: false,
        ..WebConfig::default()
    };
    let mut serve_task = tokio::spawn(server::serve(
        config,
        handles,
        "walked-port-readiness".to_string(),
        server::ServeOptions::default(),
    ));

    if std::env::var_os(TOKEN_FAILURE_FIXTURE_ENV).is_some() {
        let result = tokio::time::timeout(Duration::from_secs(2), &mut serve_task)
            .await
            .expect("token setup failure must return promptly")
            .expect("configured server task must not panic");
        assert!(result.is_err(), "an unusable token path must fail startup");
        assert!(
            sink.launch_endpoint().is_none(),
            "token setup failure must not publish readiness"
        );
        return;
    }
    if std::env::var_os(REGISTRY_FAILURE_FIXTURE_ENV).is_some() {
        let result = tokio::time::timeout(Duration::from_secs(2), &mut serve_task)
            .await
            .expect("registry setup failure must return promptly")
            .expect("configured server task must not panic");
        assert!(
            result.is_err(),
            "an unusable registry path must fail startup"
        );
        assert!(
            sink.launch_endpoint().is_none(),
            "registry failure must not publish readiness"
        );
        let notice = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                match event_rx.recv().await {
                    Some(RemoteEvent::Notice(message)) if message.contains("registry") => {
                        break message
                    }
                    Some(_) => {}
                    None => panic!("registry failure notice channel closed"),
                }
            }
        })
        .await
        .expect("registry failure notice timed out");
        assert!(notice.contains("registry"));
        return;
    }

    tokio::time::timeout(Duration::from_secs(5), launch_endpoint_rx.changed())
        .await
        .expect("configured web server did not publish its authenticated endpoint")
        .expect("configured web server readiness channel closed before publication");
    assert!(
        !serve_task.is_finished(),
        "configured web server exited while publishing readiness"
    );
    let endpoint = launch_endpoint_rx
        .borrow()
        .clone()
        .expect("readiness notification must carry an authenticated endpoint");

    let entry = server::registry::list_and_gc()
        .into_iter()
        .find(|entry| entry.pid == std::process::id())
        .expect("configured server must publish its live registry entry");
    let resume_url = vellum_fe::launcher::session_lifecycle::resume_url(
        &entry,
        vellum_fe::config::profiles::LaunchWebClient::Despana,
    )
    .expect("registry entry must reopen with the server's pairing token");
    assert!(
        resume_url.starts_with(&format!(
            "http://127.0.0.1:{}/despana#token=",
            endpoint.bound_port()
        )),
        "resume must use the actual walked port: {resume_url}"
    );
    assert!(
        resume_url.ends_with(endpoint.token()),
        "resume must pair with the same token the server installed"
    );

    assert_ne!(
        endpoint.bound_port(),
        base_port,
        "an occupied unpinned base port must be walked"
    );
    let addr = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, endpoint.bound_port()));
    let health = http_get(addr, "/health").await;
    assert!(health.contains("200"), "health: {health}");
    assert!(health.ends_with("ok"), "health body: {health}");

    // Readiness must describe the same auth state the live server installed,
    // not a second token-file read that merely happens to agree most days.
    let mut client = WsClient::connect_with_token(addr, endpoint.token()).await;
    let hello = read_json_timeout(&mut client).await;
    assert_eq!(hello["t"], "hello", "published token must authenticate");

    serve_task.abort();
    let _ = (&mut serve_task).await;
    server::registry::remove_entry();
}

struct FixtureProcess {
    child: Option<Child>,
    addr: std::net::SocketAddr,
}

impl FixtureProcess {
    async fn spawn(marker: &str) -> Self {
        // Reserve an ephemeral port long enough to learn its number. The
        // child binds it immediately; readiness polling below catches a rare
        // bind race as a child exit instead of hanging the suite.
        let reservation = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("reserve fixture port");
        let addr = reservation.local_addr().expect("fixture address");
        drop(reservation);

        let child = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("--ignored")
            .arg("--exact")
            .arg("process_isolation_fixture_host")
            .arg("--nocapture")
            .env(FIXTURE_HOST_ENV, marker)
            .env(FIXTURE_PORT_ENV, addr.port().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn fixture server process");
        let mut fixture = Self {
            child: Some(child),
            addr,
        };
        fixture.wait_until_ready().await;
        fixture
    }

    async fn wait_until_ready(&mut self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if TcpStream::connect(self.addr).await.is_ok() {
                return;
            }
            if let Some(status) = self
                .child
                .as_mut()
                .expect("fixture child")
                .try_wait()
                .expect("inspect fixture child")
            {
                panic!("fixture server exited before readiness: {status}");
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "fixture server at {} did not become ready",
                self.addr
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for FixtureProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

fn reserve_walkable_port() -> std::net::TcpListener {
    loop {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("reserve ephemeral port");
        if listener.local_addr().expect("reservation address").port() <= u16::MAX - 20 {
            return listener;
        }
    }
}

fn styled(text: &str, stream: &str) -> Arc<StyledLine> {
    Arc::new(StyledLine {
        segments: vec![TextSegment::plain(text)],
        stream: stream.to_string(),
        timestamp: None,
    })
}

async fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
    http_request(addr, &format!("GET {path}"), &[], "").await
}

async fn http_stop(addr: std::net::SocketAddr, token: &str, instance: &str) -> String {
    let authorization = format!("Bearer {token}");
    http_request(
        addr,
        "POST /api/v1/session/stop",
        &[
            ("Authorization", &authorization),
            ("X-Vellum-Instance", instance),
        ],
        "",
    )
    .await
}

async fn http_exit_logout(addr: std::net::SocketAddr, token: &str, instance: &str) -> String {
    let authorization = format!("Bearer {token}");
    http_request(
        addr,
        "POST /api/v1/session/exit-logout",
        &[
            ("Authorization", &authorization),
            ("X-Vellum-Instance", instance),
        ],
        "",
    )
    .await
}

async fn http_request(
    addr: std::net::SocketAddr,
    request_line: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let extra_headers = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let req = format!(
        "{request_line} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n{extra_headers}\r\n{body}",
        body.len()
    );
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
    /// Handshake + pairing auth (the normal path for every test client).
    async fn connect(addr: std::net::SocketAddr) -> Self {
        Self::connect_with_token(addr, TEST_TOKEN).await
    }

    /// Handshake + auth with an explicitly supplied token. Configured-server
    /// readiness tests use this to prove the published endpoint is coherent.
    async fn connect_with_token(addr: std::net::SocketAddr, token: &str) -> Self {
        let mut client = Self::connect_unauthenticated(addr).await;
        client
            .send_text(&format!(r#"{{"t":"auth","d":{{"token":"{token}"}}}}"#))
            .await;
        client
    }

    /// Handshake only — for tests exercising the auth gate itself.
    async fn connect_unauthenticated(addr: std::net::SocketAddr) -> Self {
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
        self.stream
            .read_exact(&mut header)
            .await
            .expect("frame header");
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
        self.stream
            .read_exact(&mut payload)
            .await
            .expect("frame payload");
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
        frame.extend(bytes.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
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

/// Connect, drain hello (answering with resume seq) and the macros message
/// that follows the snapshot; return the client and the snapshot message.
async fn connect_and_sync(
    addr: std::net::SocketAddr,
    resume_seq: u64,
) -> (WsClient, serde_json::Value) {
    let (client, _hello, snapshot) = connect_and_sync_with_hello(addr, resume_seq).await;
    (client, snapshot)
}

async fn connect_and_sync_with_hello(
    addr: std::net::SocketAddr,
    resume_seq: u64,
) -> (WsClient, serde_json::Value, serde_json::Value) {
    let mut client = WsClient::connect(addr).await;
    let hello = read_json_timeout(&mut client).await;
    assert_eq!(hello["t"], "hello");
    client.send_resume(resume_seq).await;
    let snapshot = read_json_timeout(&mut client).await;
    assert_eq!(snapshot["t"], "snapshot");
    let macros = read_json_timeout(&mut client).await;
    assert_eq!(macros["t"], "macros");
    let wheels = read_json_timeout(&mut client).await;
    assert_eq!(wheels["t"], "wheels");
    (client, hello, snapshot)
}

#[test]
fn configured_server_reports_walked_port_and_serves_health() {
    let reservation = reserve_walkable_port();
    let base_port = reservation
        .local_addr()
        .expect("reservation address")
        .port();
    let data_dir = tempfile::tempdir().expect("temporary VELLUM_FE_DIR");

    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("walked_port_readiness_fixture_host")
        .arg("--nocapture")
        .env(WALKED_PORT_FIXTURE_ENV, "1")
        .env(FIXTURE_PORT_ENV, base_port.to_string())
        .env("VELLUM_FE_DIR", data_dir.path())
        .env("VELLUM_FE_RUNTIME_DIR", data_dir.path().join("runtime"))
        .output()
        .expect("spawn walked-port readiness fixture");

    assert!(
        output.status.success(),
        "walked-port readiness fixture failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn configured_server_token_failure_never_publishes_readiness() {
    let reservation = reserve_walkable_port();
    let base_port = reservation
        .local_addr()
        .expect("reservation address")
        .port();
    let data_dir = tempfile::tempdir().expect("temporary parent directory");
    let unusable_data_dir = data_dir.path().join("not-a-directory");
    std::fs::write(&unusable_data_dir, b"blocks directory creation")
        .expect("create unusable VELLUM_FE_DIR path");

    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("walked_port_readiness_fixture_host")
        .arg("--nocapture")
        .env(WALKED_PORT_FIXTURE_ENV, "1")
        .env(TOKEN_FAILURE_FIXTURE_ENV, "1")
        .env(FIXTURE_PORT_ENV, base_port.to_string())
        .env("VELLUM_FE_DIR", &unusable_data_dir)
        .output()
        .expect("spawn token-failure readiness fixture");

    assert!(
        output.status.success(),
        "token-failure readiness fixture failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn configured_server_registry_failure_never_publishes_readiness() {
    let reservation = reserve_walkable_port();
    let base_port = reservation
        .local_addr()
        .expect("reservation address")
        .port();
    let data_dir = tempfile::tempdir().expect("temporary data directory");
    let runtime_file = data_dir.path().join("not-a-runtime-directory");
    std::fs::write(&runtime_file, b"blocks runtime directory creation")
        .expect("create unusable runtime path");

    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("walked_port_readiness_fixture_host")
        .arg("--nocapture")
        .env(WALKED_PORT_FIXTURE_ENV, "1")
        .env(REGISTRY_FAILURE_FIXTURE_ENV, "1")
        .env(FIXTURE_PORT_ENV, base_port.to_string())
        .env("VELLUM_FE_DIR", data_dir.path())
        .env("VELLUM_FE_RUNTIME_DIR", &runtime_file)
        .output()
        .expect("spawn registry-failure readiness fixture");

    assert!(
        output.status.success(),
        "registry-failure readiness fixture failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[tokio::test]
async fn pinned_occupied_port_failure_never_publishes_readiness() {
    let reservation = reserve_walkable_port();
    let port = reservation
        .local_addr()
        .expect("reservation address")
        .port();
    let (sink, handles, mut event_rx) = RemoteSink::new(100);
    let config = WebConfig {
        enabled: true,
        multiaccount: false,
        port,
        bind: std::net::Ipv4Addr::LOCALHOST.to_string(),
        pinned: true,
        ..WebConfig::default()
    };

    let result = tokio::time::timeout(
        Duration::from_secs(2),
        server::serve(
            config,
            handles,
            "pinned-port-failure".to_string(),
            server::ServeOptions::default(),
        ),
    )
    .await
    .expect("pinned bind failure must return promptly");

    assert!(result.is_err(), "an occupied pinned port must fail");
    assert_eq!(
        sink.launch_endpoint().map(|endpoint| endpoint.bound_port()),
        None,
        "a failed bind must not publish a launchable port"
    );
    let notice = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
        .await
        .expect("bind failure Notice must arrive")
        .expect("bind failure Notice channel closed early");
    match notice {
        RemoteEvent::Notice(message) => assert!(
            message.contains("pinned port") && message.contains("is taken"),
            "unexpected bind failure Notice: {message}"
        ),
        other => panic!("expected bind failure Notice, got {other:?}"),
    }
    assert!(
        event_rx.recv().await.is_none(),
        "the failed server must close its event channel"
    );
}

#[tokio::test]
async fn authenticated_stop_is_delivered_for_recoverable_but_not_connected_sessions() {
    let (mut sink, handles, mut event_rx) = RemoteSink::new(10);
    let instance = handles.session.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = server::serve_listener(
            listener,
            handles,
            TEST_TOKEN.to_string(),
            server::ServeOptions::default(),
        )
        .await;
    });
    sink.set_session_control(true);
    sink.set_session_state(RemoteSessionInfo {
        state: SessionState::Idle,
        ..Default::default()
    });

    let denied = http_stop(addr, "wrong-token", &instance).await;
    assert!(denied.starts_with("HTTP/1.1 403"), "got: {denied}");
    assert!(event_rx.try_recv().is_err());

    let stale = http_stop(addr, TEST_TOKEN, "stale-instance").await;
    assert!(stale.starts_with("HTTP/1.1 409"), "got: {stale}");
    assert!(event_rx.try_recv().is_err());

    let accepted = http_stop(addr, TEST_TOKEN, &instance).await;
    assert!(accepted.starts_with("HTTP/1.1 202"), "got: {accepted}");
    assert!(matches!(
        event_rx.recv().await,
        Some(RemoteEvent::SessionStop)
    ));

    for recoverable in [
        SessionState::Authenticating,
        SessionState::Connecting,
        SessionState::Reconnecting,
        SessionState::Disconnected,
    ] {
        sink.set_session_state(RemoteSessionInfo {
            state: recoverable,
            ..Default::default()
        });
        let accepted = http_stop(addr, TEST_TOKEN, &instance).await;
        assert!(accepted.starts_with("HTTP/1.1 202"), "got: {accepted}");
        assert!(matches!(
            event_rx.recv().await,
            Some(RemoteEvent::SessionStop)
        ));
    }

    sink.set_session_state(RemoteSessionInfo {
        state: SessionState::Connected,
        ..Default::default()
    });
    let active = http_stop(addr, TEST_TOKEN, &instance).await;
    assert!(active.starts_with("HTTP/1.1 409"), "got: {active}");
    assert!(event_rx.try_recv().is_err());
}

#[tokio::test]
async fn authenticated_exit_logout_requires_the_exact_connected_session() {
    let (mut sink, handles, mut event_rx) = RemoteSink::new(10);
    let instance = handles.session.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = server::serve_listener(
            listener,
            handles,
            TEST_TOKEN.to_string(),
            server::ServeOptions::default(),
        )
        .await;
    });

    sink.set_session_control(true);
    sink.set_session_state(RemoteSessionInfo {
        state: SessionState::Connected,
        ..Default::default()
    });

    let denied = http_exit_logout(addr, "wrong-token", &instance).await;
    assert!(denied.starts_with("HTTP/1.1 403"), "got: {denied}");
    assert!(event_rx.try_recv().is_err());

    let stale = http_exit_logout(addr, TEST_TOKEN, "stale-instance").await;
    assert!(stale.starts_with("HTTP/1.1 409"), "got: {stale}");
    assert!(event_rx.try_recv().is_err());

    sink.set_session_state(RemoteSessionInfo {
        state: SessionState::Idle,
        ..Default::default()
    });
    let idle = http_exit_logout(addr, TEST_TOKEN, &instance).await;
    assert!(idle.starts_with("HTTP/1.1 409"), "got: {idle}");
    assert!(event_rx.try_recv().is_err());

    sink.set_session_state(RemoteSessionInfo {
        state: SessionState::Connected,
        ..Default::default()
    });
    let accepted = http_exit_logout(addr, TEST_TOKEN, &instance).await;
    assert!(accepted.starts_with("HTTP/1.1 202"), "got: {accepted}");
    assert!(matches!(
        event_rx.recv().await,
        Some(RemoteEvent::SessionExitLogout)
    ));
}

#[tokio::test]
async fn status_only_server_accepts_authenticated_exit_logout() {
    let (mut sink, handles, mut event_rx) = RemoteSink::new(10);
    let instance = handles.session.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = server::serve_listener(
            listener,
            handles,
            TEST_TOKEN.to_string(),
            server::ServeOptions {
                status_only: true,
                ..Default::default()
            },
        )
        .await;
    });
    sink.set_session_control(true);
    sink.set_session_state(RemoteSessionInfo {
        state: SessionState::Connected,
        ..Default::default()
    });

    let accepted = http_exit_logout(addr, TEST_TOKEN, &instance).await;
    assert!(accepted.starts_with("HTTP/1.1 202"), "got: {accepted}");
    assert!(matches!(
        event_rx.recv().await,
        Some(RemoteEvent::SessionExitLogout)
    ));
}

#[tokio::test]
async fn closing_a_browser_never_requests_game_disconnect_or_process_stop() {
    let (_sink, mut event_rx, addr) = start_server(10).await;
    let client = WsClient::connect(addr).await;
    drop(client);
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(event_rx.try_recv().is_err());
}

#[tokio::test]
async fn explicit_exit_logout_frame_reaches_runtime_without_becoming_a_command() {
    let (_sink, mut event_rx, addr) = start_server(10).await;
    let (mut client, _) = connect_and_sync(addr, 0).await;

    client.send_text(r#"{"t":"exit_logout","d":{}}"#).await;

    assert!(matches!(
        event_rx.recv().await,
        Some(RemoteEvent::SessionExitLogout)
    ));
    assert!(event_rx.try_recv().is_err());
}

#[tokio::test]
async fn health_and_static_assets_are_served() {
    let (_sink, _event_rx, addr) = start_server(100).await;

    let health = http_get(addr, "/health").await;
    assert!(health.contains("200"), "health: {health}");
    assert!(health.ends_with("ok"), "health body: {health}");
    assert!(
        health.contains("access-control-allow-origin: *"),
        "dashboard health probes are cross-port: {health}"
    );

    // "/" is the multi-session dashboard; the game client lives at /play.
    let dashboard = http_get(addr, "/").await;
    assert!(dashboard.contains("200"));
    assert!(dashboard.contains("Pick a session"));

    let index = http_get(addr, "/play").await;
    assert!(index.contains("200"));
    assert!(index.contains("VellumFE"));
    assert!(index.contains("cmd-suggestion"));

    let despana = http_get(addr, "/despana").await;
    assert!(despana.contains("200"));
    assert!(despana.contains("text/html"));
    assert!(despana.contains("Vellum Despana"));
    assert!(!despana.contains("VellumFE — Despana"));
    assert!(despana.contains("data-despana-desktop"));
    assert!(despana.contains("href=\"/despana/app.css\""));
    assert!(despana.contains("src=\"/despana/app.js\""));
    for zone in ["top", "bottom", "left", "right", "center"] {
        assert!(
            despana.contains(&format!("data-zone=\"{zone}\"")),
            "Despana shell is missing the {zone} workspace zone"
        );
    }
    assert!(despana.contains("workspace-menu-button"));
    assert!(despana.contains("font-scale"));
    assert!(despana.contains("map-mode-classic"));
    assert!(despana.contains("map-mode-local"));
    assert!(despana.contains("map-selector"));
    for direction in ["up", "down", "out"] {
        assert!(
            despana.contains(&format!("data-direction=\"{direction}\"")),
            "Despana compass is missing the {direction} direction"
        );
    }

    let despana_app = http_get(addr, "/despana/app.js").await;
    assert!(despana_app.contains("text/javascript"));
    assert!(despana_app.contains("DesktopSession"));
    assert!(despana_app.contains("Vellum Despana workspace error"));
    assert!(!despana_app.contains("VellumFE — Despana"));
    assert!(despana_app.contains("/api/v1/maps/classic"));

    let despana_session = http_get(addr, "/despana/session.js").await;
    assert!(despana_session.contains("text/javascript"));
    assert!(despana_session.contains("export class DesktopSession"));

    let despana_inventory_refresh = http_get(addr, "/despana/inventory-refresh.js").await;
    assert!(despana_inventory_refresh.contains("text/javascript"));
    assert!(despana_inventory_refresh.contains("export class InventoryRefreshTracker"));

    let despana_inventory_tree = http_get(addr, "/despana/inventory-tree.js").await;
    assert!(despana_inventory_tree.contains("text/javascript"));
    assert!(despana_inventory_tree.contains("export function projectInventoryItems"));
    let despana_font_scale = http_get(addr, "/despana/font-scale.js").await;
    assert!(despana_font_scale.contains("text/javascript"));
    assert!(despana_font_scale.contains("export function normalizeFontScale"));

    let despana_interactions = http_get(addr, "/despana/interactions.js").await;
    assert!(despana_interactions.contains("text/javascript"));
    assert!(despana_interactions.contains("export class DesktopInteractionCoordinator"));

    let despana_layout = http_get(addr, "/despana/layout.js").await;
    assert!(despana_layout.contains("text/javascript"));
    assert!(despana_layout.contains("export class WorkspaceLayout"));

    let despana_persistence = http_get(addr, "/despana/workspace-persistence.js").await;
    assert!(despana_persistence.contains("text/javascript"));
    assert!(despana_persistence.contains("export function createDesktopWorkspaceStore"));
    assert!(!despana_persistence.contains("document.cookie"));

    let workspace_denied = http_get(addr, "/api/v1/presentations/despana/workspace").await;
    assert!(workspace_denied.contains("403"));
    let workspace_without_character = http_request(
        addr,
        "GET /api/v1/presentations/despana/workspace",
        &[("Authorization", "Bearer test-token")],
        "",
    )
    .await;
    assert!(workspace_without_character.contains("409"));

    let despana_map = http_get(addr, "/despana/map.js").await;
    assert!(despana_map.contains("text/javascript"));
    assert!(despana_map.contains("export class DesktopMapViewport"));

    let classic_maps_denied = http_get(addr, "/api/v1/maps/classic").await;
    assert!(classic_maps_denied.contains("403"));
    let classic_maps = http_get(addr, &format!("/api/v1/maps/classic?token={TEST_TOKEN}")).await;
    assert!(classic_maps.contains("200"));
    assert!(classic_maps.contains("application/json"));
    let missing_classic = http_get(
        addr,
        &format!("/api/v1/maps/classic/not-a-real-map.png?token={TEST_TOKEN}"),
    )
    .await;
    assert!(missing_classic.contains("404"));

    let despana_workspace = http_get(addr, "/despana/workspace.js").await;
    assert!(despana_workspace.contains("text/javascript"));
    assert!(despana_workspace.contains("export class DesktopWorkspace"));

    let despana_css = http_get(addr, "/despana/app.css").await;
    assert!(despana_css.contains("text/css"));
    assert!(despana_css.contains("--despana-amber"));

    // The multi-account status wall: dials every session's /ws in watch
    // mode client-side, so serving the page is all the server does.
    let wall = http_get(addr, "/characters").await;
    assert!(wall.contains("200"));
    assert!(
        wall.contains("mode: \"watch\""),
        "the wall subscribes as a watcher"
    );
    assert!(
        wall.contains("subscribe") && wall.contains("resume"),
        "handshake mirrors the hub: auth, subscribe watch, resume"
    );

    let sessions = http_get(addr, "/sessions").await;
    assert!(sessions.contains("application/json"));

    let js = http_get(addr, "/app.js").await;
    assert!(js.contains("text/javascript"));
    assert!(js.contains("updateCommandSuggestion"));

    let css = http_get(addr, "/app.css").await;
    assert!(css.contains("text/css"));
    assert!(css.contains("#cmd-suggestion"));

    // PWA shell (Phase 4)
    let manifest = http_get(addr, "/manifest.webmanifest").await;
    assert!(manifest.contains("application/manifest+json"));
    assert!(manifest.contains("\"display\": \"standalone\""));

    let sw = http_get(addr, "/sw.js").await;
    assert!(sw.contains("text/javascript"));

    let icon = http_get(addr, "/icon.svg").await;
    assert!(icon.contains("image/svg+xml"));
}

#[tokio::test]
async fn classic_map_filesystem_authority_is_isolated_per_server() {
    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();
    std::fs::write(first_dir.path().join("aster.png"), b"aster-map").unwrap();
    std::fs::write(second_dir.path().join("briar.png"), b"briar-map").unwrap();

    let first_catalog = Arc::new(ClassicMapCatalog::new());
    let second_catalog = Arc::new(ClassicMapCatalog::new());
    first_catalog.reload_from_dir(Some(first_dir.path()));
    second_catalog.reload_from_dir(Some(second_dir.path()));
    let (_first_sink, _first_events, first_addr) =
        start_server_with_catalog(10, first_catalog).await;
    let (_second_sink, _second_events, second_addr) =
        start_server_with_catalog(10, second_catalog).await;

    let first = http_get(
        first_addr,
        &format!("/api/v1/maps/classic?token={TEST_TOKEN}"),
    )
    .await;
    let second = http_get(
        second_addr,
        &format!("/api/v1/maps/classic?token={TEST_TOKEN}"),
    )
    .await;
    assert!(first.contains("aster.png"));
    assert!(!first.contains("briar.png"));
    assert!(second.contains("briar.png"));
    assert!(!second.contains("aster.png"));

    let first_image = http_get(
        first_addr,
        &format!("/api/v1/maps/classic/aster.png?token={TEST_TOKEN}"),
    )
    .await;
    let crossed_image = http_get(
        first_addr,
        &format!("/api/v1/maps/classic/briar.png?token={TEST_TOKEN}"),
    )
    .await;
    assert!(first_image.contains("aster-map"));
    assert!(crossed_image.contains("404"));
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
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));
    let vitals = read_json_timeout(&mut client).await;
    assert_eq!(vitals["t"], "vitals");
    assert_eq!(vitals["d"]["health"], 42);
}

#[tokio::test]
async fn parsed_character_sheet_identity_flows_through_snapshot_and_live_expr_wins() {
    let (mut sink, _event_rx, addr) = start_server(100).await;
    let mut gs = GameState::new();
    gs.character
        .parse_line("Name: Briar Sage Race: Human  Profession: Wizard");
    gs.character
        .parse_line("Gender: Female    Age: 40    Expr: 6,400,000    Level:  89");
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    assert_eq!(snapshot["d"]["char_info"]["profession"], "Wizard");
    assert_eq!(snapshot["d"]["char_info"]["level"], "89");

    gs.gs4_experience.update_level("Level: 90".to_string());
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "charinfo");
    assert_eq!(delta["d"]["profession"], "Wizard");
    assert_eq!(delta["d"]["level"], "90");
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
async fn two_server_processes_keep_sessions_state_and_commands_isolated() {
    let mut aster = FixtureProcess::spawn("Aster").await;
    let mut briar = FixtureProcess::spawn("Briar").await;

    for fixture in [&aster, &briar] {
        let health = http_get(fixture.addr, "/health").await;
        assert!(health.contains("200"), "health: {health}");
        assert!(health.ends_with("ok"), "health body: {health}");

        let despana = http_get(fixture.addr, "/despana").await;
        assert!(despana.contains("200"), "Despana: {despana}");
        assert!(despana.contains("data-despana-desktop"));
    }

    let (mut aster_client, aster_hello, aster_snapshot) =
        connect_and_sync_with_hello(aster.addr, 0).await;
    let (mut briar_client, briar_hello, briar_snapshot) =
        connect_and_sync_with_hello(briar.addr, 0).await;

    assert_ne!(
        aster_hello["d"]["session"], briar_hello["d"]["session"],
        "independent processes must advertise independent resume epochs"
    );
    assert_eq!(aster_hello["d"]["character"], "Aster");
    assert_eq!(briar_hello["d"]["character"], "Briar");
    assert_eq!(aster_snapshot["d"]["character"], "Aster");
    assert_eq!(briar_snapshot["d"]["character"], "Briar");
    assert_eq!(
        aster_snapshot["d"]["room"]["name"], "Aster Room",
        "Aster receives only Aster's state"
    );
    assert_eq!(
        briar_snapshot["d"]["room"]["name"], "Briar Room",
        "Briar receives only Briar's state"
    );
    assert_eq!(
        aster_snapshot["d"]["text"][0]["line"]["segments"][0]["text"],
        "Aster story marker"
    );
    assert_eq!(
        briar_snapshot["d"]["text"][0]["line"]["segments"][0]["text"],
        "Briar story marker"
    );

    aster_client
        .send_text(r#"{"t":"cmd","d":{"text":"look aster"}}"#)
        .await;
    let aster_ack = read_json_timeout(&mut aster_client).await;
    assert_eq!(aster_ack["t"], "text");
    assert_eq!(
        aster_ack["d"]["line"]["segments"][0]["text"],
        "Aster ack: look aster"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(150), briar_client.read_json())
            .await
            .is_err(),
        "Aster's command acknowledgment leaked into Briar's process"
    );

    briar_client
        .send_text(r#"{"t":"cmd","d":{"text":"look briar"}}"#)
        .await;
    let briar_ack = read_json_timeout(&mut briar_client).await;
    assert_eq!(briar_ack["t"], "text");
    assert_eq!(
        briar_ack["d"]["line"]["segments"][0]["text"],
        "Briar ack: look briar"
    );

    // Losing one character runtime cannot take the other server, socket, or
    // command loop down with it.
    aster.stop();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), TcpStream::connect(aster.addr))
            .await
            .expect("closed fixture connection check timed out")
            .is_err(),
        "stopped Aster fixture still accepts connections"
    );

    let health = http_get(briar.addr, "/health").await;
    assert!(
        health.ends_with("ok"),
        "Briar health after Aster exit: {health}"
    );
    briar_client
        .send_text(r#"{"t":"cmd","d":{"text":"still here"}}"#)
        .await;
    let still_alive = read_json_timeout(&mut briar_client).await;
    assert_eq!(still_alive["t"], "text");
    assert_eq!(
        still_alive["d"]["line"]["segments"][0]["text"],
        "Briar ack: still here"
    );

    // Explicit stop exercises the idempotent cleanup path; Drop remains the
    // panic-safe guarantee for either process on every earlier assertion.
    briar.stop();
}

#[tokio::test]
async fn client_cmd_arrives_as_remote_event() {
    let (_sink, mut event_rx, addr) = start_server(100).await;

    let (mut client, _) = connect_and_sync(addr, 0).await;
    client.send_text(r#"{"t":"cmd","d":{"text":"look"}}"#).await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out waiting for remote event")
        .expect("event channel open");
    let RemoteEvent::Command { client_id, text } = event else {
        panic!("expected Command event")
    };
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
    let RemoteEvent::Command {
        client_id: second_client_id,
        text,
    } = event
    else {
        panic!("expected Command event")
    };
    assert_eq!(text, "second");
    assert_eq!(second_client_id, client_id, "one socket keeps one address");
}

#[tokio::test]
async fn open_url_reply_routes_only_to_the_commanding_browser() {
    let (mut sink, mut event_rx, addr) = start_server(100).await;
    let (mut requester, _) = connect_and_sync(addr, 0).await;
    let (mut other, _) = connect_and_sync(addr, 0).await;

    requester
        .send_text(r#"{"t":"cmd","d":{"text":"GOALS"}}"#)
        .await;
    let event = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out waiting for GOALS")
        .expect("event channel open");
    let RemoteEvent::Command { client_id, text } = event else {
        panic!("expected addressed Command event")
    };
    assert_eq!(text, "GOALS");

    sink.push_open_url(
        client_id,
        "https://www.play.net/gs4/play/cm/loader.asp?ticket=addressed".to_string(),
    );
    sink.push_text("main", styled("after-open-url", "main"));

    let opened = read_json_timeout(&mut requester).await;
    assert_eq!(opened["t"], "open_url");
    assert_eq!(
        opened["d"]["url"],
        "https://www.play.net/gs4/play/cm/loader.asp?ticket=addressed"
    );
    assert!(opened["d"].get("client_id").is_none());

    let next_for_other = read_json_timeout(&mut other).await;
    assert_eq!(
        next_for_other["t"], "text",
        "other browsers and watchers must never receive the URL"
    );
    assert_eq!(
        next_for_other["d"]["line"]["segments"][0]["text"],
        "after-open-url"
    );
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
async fn macros_flow_definitions_out_taps_in() {
    let (mut sink, mut event_rx, addr) = start_server(100).await;

    let macros_config: vellum_fe::config::MacrosConfig = toml::from_str(
        r##"
        [[group]]
        name = "Town"
        [[group.button]]
        label = "Look"
        command = "look"
        [[group.button]]
        label = "Travel"
        [[group.button.option]]
        label = "Bank"
        command = ";go2 bank"
        [[group.button]]
        label = "go"
        command = "go"
        insert = true
        [[floating]]
        label = "Atk"
        command = ";bigshot"
        "##,
    )
    .unwrap();
    sink.set_macros(&macros_config);

    let mut client = WsClient::connect(addr).await;
    assert_eq!(read_json_timeout(&mut client).await["t"], "hello");
    client.send_resume(0).await;
    assert_eq!(read_json_timeout(&mut client).await["t"], "snapshot");

    // Definitions arrive after the snapshot: ids and labels, no commands.
    let macros = read_json_timeout(&mut client).await;
    assert_eq!(macros["t"], "macros");
    let d = &macros["d"];
    assert_eq!(d["groups"][0]["name"], "Town");
    assert_eq!(d["groups"][0]["buttons"][0]["id"], "g:0:b:0");
    assert_eq!(
        d["groups"][0]["buttons"][1]["options"][0]["id"],
        "g:0:b:1:o:0"
    );
    assert_eq!(d["floating"][0]["id"], "f:0");
    assert!(
        !macros.to_string().contains(";go2 bank"),
        "commands must never reach the client"
    );
    // Type-in buttons are the exception: the client needs their text to
    // put it in the input box, so it ships with the definition.
    assert_eq!(d["groups"][0]["buttons"][2]["insert"], true);
    assert_eq!(d["groups"][0]["buttons"][2]["command"], "go");
    assert_eq!(d["groups"][0]["buttons"][0]["insert"], false);

    // Wheel definitions follow the macros on connect.
    assert_eq!(read_json_timeout(&mut client).await["t"], "wheels");

    // Stale-client guard: a tap id pointing at an insert button must not
    // resolve to an executable command server-side.
    assert_eq!(macros_config.resolve("g:0:b:2"), None);

    // A tap comes back as an id-only event.
    client
        .send_text(r#"{"t":"macro","d":{"id":"g:0:b:1:o:0"}}"#)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::Macro { id } = event else {
        panic!("expected Macro event");
    };
    assert_eq!(id, "g:0:b:1:o:0");
    // ...which core resolves back to the command.
    assert_eq!(macros_config.resolve(&id), Some(";go2 bank"));

    // A reload pushes fresh definitions to connected clients as a delta.
    sink.set_macros(&vellum_fe::config::MacrosConfig::default());
    let update = read_json_timeout(&mut client).await;
    assert_eq!(update["t"], "macros");
    assert_eq!(update["d"]["groups"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn entities_flow_in_snapshot_and_deltas() {
    use vellum_fe::core::remote::{RemoteRoomEntity, RemoteStateSnapshot};
    let (mut sink, _event_rx, addr) = start_server(100).await;

    let entity = |id: &str, label: &str, noun: &str| RemoteRoomEntity {
        id: id.to_string(),
        label: label.to_string(),
        noun: noun.to_string(),
    };
    let mut snap = RemoteStateSnapshot::default();
    snap.entities.creatures = vec![entity("111", "a muddy hog (stunned)", "hog")];
    snap.portals = vec!["go gate".to_string()];
    sink.flush_state(snap.clone());

    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    assert_eq!(snapshot["d"]["portals"][0], "go gate");
    assert_eq!(snapshot["d"]["entities"]["creatures"][0]["id"], "111");
    assert_eq!(
        snapshot["d"]["entities"]["creatures"][0]["label"],
        "a muddy hog (stunned)"
    );
    assert_eq!(snapshot["d"]["entities"]["creatures"][0]["noun"], "hog");
    assert_eq!(
        snapshot["d"]["entities"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // A room change flows as coalesced entities + portals deltas.
    snap.entities.players = vec![entity("444", "Testy", "Testy")];
    snap.portals = vec!["go gate".to_string(), "climb ladder".to_string()];
    sink.flush_state(snap);
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "entities");
    assert_eq!(delta["d"]["players"][0]["id"], "444");
    assert_eq!(delta["d"]["creatures"][0]["id"], "111");
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "portals");
    assert_eq!(delta["d"][1], "climb ladder");
}

#[tokio::test]
async fn room_description_and_spellbook_flow_in_snapshot_and_deltas() {
    use vellum_fe::core::remote::RemoteStateSnapshot;
    let (mut sink, _event_rx, addr) = start_server(100).await;

    // A styled line helper: room prose and the spellbook ride the wire as
    // StyledLine (segments with color/links), the same shape as text deltas.
    let styled = |text: &str| vellum_fe::data::widget::StyledLine {
        segments: vec![vellum_fe::data::widget::TextSegment {
            text: text.to_string(),
            ..Default::default()
        }],
        stream: "room".to_string(),
        timestamp: None,
    };

    // Initial state: a room with prose and two active spells.
    let mut snap = RemoteStateSnapshot::default();
    snap.room_name = Some("Town Square".to_string());
    snap.room_description = vec![styled("A fountain bubbles at the center.")];
    snap.spellbook = vec![
        styled("Elemental Defense III (503)   00:14:59"),
        styled("Mana Leech (516)   00:29:42"),
    ];
    sink.flush_state(snap.clone());

    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    // Room prose rides the room payload as styled lines; spellbook likewise.
    // The phone renders these through the same renderLine path as text, so
    // the wire carries `segments`, not bare strings.
    assert_eq!(
        snapshot["d"]["room"]["description"][0]["segments"][0]["text"],
        "A fountain bubbles at the center."
    );
    assert_eq!(
        snapshot["d"]["spellbook"][0]["segments"][0]["text"],
        "Elemental Defense III (503)   00:14:59"
    );
    assert_eq!(snapshot["d"]["spellbook"].as_array().unwrap().len(), 2);

    // Walking to a new room changes both: a `room` delta carries the new
    // prose, and a `spells` delta carries the updated list.
    snap.room_name = Some("Dark Alcove".to_string());
    snap.room_description = vec![styled("Shadows pool in the corners.")];
    snap.spellbook = vec![styled("Elemental Defense III (503)   00:13:12")];
    sink.flush_state(snap);

    // Deltas arrive in flush order (room before spells).
    let room = read_json_timeout(&mut client).await;
    assert_eq!(room["t"], "room");
    assert_eq!(room["d"]["name"], "Dark Alcove");
    assert_eq!(
        room["d"]["description"][0]["segments"][0]["text"],
        "Shadows pool in the corners."
    );

    let spells = read_json_timeout(&mut client).await;
    assert_eq!(spells["t"], "spells");
    assert_eq!(spells["d"].as_array().unwrap().len(), 1);
    assert_eq!(
        spells["d"][0]["segments"][0]["text"],
        "Elemental Defense III (503)   00:13:12"
    );
}

#[tokio::test]
async fn styled_inventory_flows_in_snapshot_and_replacement_delta() {
    use vellum_fe::core::remote::RemoteStateSnapshot;
    use vellum_fe::data::widget::{LinkData, StyledLine, TextSegment};

    let (mut sink, _event_rx, addr) = start_server(100).await;
    let inventory_line = |id: &str, noun: &str, text: &str| StyledLine {
        segments: vec![TextSegment {
            text: text.to_string(),
            link_data: Some(LinkData {
                exist_id: id.to_string(),
                noun: noun.to_string(),
                text: text.to_string(),
                coord: None,
            }),
            ..Default::default()
        }],
        stream: "inv".to_string(),
        timestamp: None,
    };

    let mut game_state = vellum_fe::core::GameState::new();
    game_state.inventory_received = true;
    game_state.inventory = vec![inventory_line(
        "535703780",
        "backpack",
        "a patchwork backpack",
    )];
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));

    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    assert_eq!(snapshot["d"]["inventory"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["d"]["inventory_received"], true);
    assert_eq!(
        snapshot["d"]["inventory"][0]["segments"][0]["link_data"]["exist_id"],
        "535703780"
    );
    assert_eq!(
        snapshot["d"]["inventory"][0]["segments"][0]["link_data"]["noun"],
        "backpack"
    );

    game_state.inventory = vec![inventory_line("42", "orb", "a crystal orb")];
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "inventory");
    assert_eq!(delta["d"].as_array().unwrap().len(), 1);
    assert_eq!(delta["d"][0]["segments"][0]["text"], "a crystal orb");
    assert_eq!(delta["d"][0]["segments"][0]["link_data"]["exist_id"], "42");

    game_state.inventory.clear();
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "inventory");
    assert_eq!(delta["d"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn authoritative_empty_inventory_receipt_flows_without_a_line_delta() {
    use vellum_fe::core::remote::RemoteStateSnapshot;

    let (mut sink, _event_rx, addr) = start_server(100).await;
    let mut game_state = GameState::new();
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));
    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    assert!(snapshot["d"].get("inventory_received").is_none());
    assert!(snapshot["d"].get("inventory").is_none());

    game_state.inventory_received = true;
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "inventory_received");
    assert_eq!(delta["d"], true);
}

#[tokio::test]
async fn inventory_tree_flows_in_snapshot_replacement_and_null_clear() {
    use vellum_fe::core::remote::RemoteStateSnapshot;
    use vellum_fe::core::state::{ManagedInventoryItem, ManagedInventoryState};

    let (mut sink, _event_rx, addr) = start_server(100).await;
    let mut game_state = GameState::new();
    game_state.managed_inventory = Some(ManagedInventoryState {
        token: "im-private-token".to_string(),
        room: "2005".to_string(),
        items: vec![
            ManagedInventoryItem {
                id: "bag".to_string(),
                relation: "worn".to_string(),
                parent: "player".to_string(),
                name: "a patchwork backpack".to_string(),
                article: "a".to_string(),
                adjective: "patchwork".to_string(),
                noun: "backpack".to_string(),
                long: Some("a patchwork backpack bound by vines".to_string()),
                weight: 5,
                encum: Some(4),
                in_max: Some(2000),
                on_max: None,
                in_encum: Some(7),
                in_selector: Some("backpack".to_string()),
                locker: false,
                familyvault: false,
                flags: vec!["closed".to_string()],
            },
            ManagedInventoryItem {
                id: "pouch".to_string(),
                relation: "in".to_string(),
                parent: "bag".to_string(),
                name: "a coal black purse".to_string(),
                article: "a".to_string(),
                adjective: "coal black".to_string(),
                noun: "purse".to_string(),
                long: None,
                weight: 3,
                encum: None,
                in_max: Some(505),
                on_max: Some(101),
                in_encum: Some(2),
                in_selector: None,
                locker: true,
                familyvault: true,
                flags: vec!["closed".to_string(), "locked".to_string()],
            },
            ManagedInventoryItem {
                id: "wand".to_string(),
                relation: "in".to_string(),
                parent: "pouch".to_string(),
                name: "an aquamarine wand".to_string(),
                article: "an".to_string(),
                adjective: "aquamarine".to_string(),
                noun: "wand".to_string(),
                weight: 1,
                ..Default::default()
            },
        ],
        complete: true,
        generation: 11,
    });
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));

    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    let tree = &snapshot["d"]["inventory_tree"];
    assert_eq!(tree["room"], "2005");
    assert_eq!(tree["complete"], true);
    assert_eq!(tree["generation"], 11);
    assert!(
        tree.get("token").is_none(),
        "request token stays core-internal"
    );
    assert_eq!(tree["items"].as_array().unwrap().len(), 3);
    assert_eq!(tree["items"][1]["relation"], "in");
    assert_eq!(tree["items"][1]["parent"], "bag");
    assert_eq!(tree["items"][1]["in_max"], 505);
    assert_eq!(tree["items"][1]["on_max"], 101);
    assert_eq!(tree["items"][1]["in_encum"], 2);
    assert_eq!(tree["items"][1]["locker"], true);
    assert_eq!(tree["items"][1]["familyvault"], true);
    assert_eq!(
        tree["items"][1]["flags"],
        serde_json::json!(["closed", "locked"])
    );
    assert_eq!(tree["items"][2]["parent"], "pouch");

    let managed = game_state.managed_inventory.as_mut().unwrap();
    managed.generation = 12;
    managed.items[1].flags = vec!["closed".to_string()];
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));
    let replacement = read_json_timeout(&mut client).await;
    assert_eq!(replacement["t"], "inventory_tree");
    assert_eq!(replacement["d"]["generation"], 12);
    assert_eq!(
        replacement["d"]["items"][1]["flags"],
        serde_json::json!(["closed"])
    );

    game_state.managed_inventory = None;
    sink.flush_state(RemoteStateSnapshot::from_game_state(&game_state, &[]));
    let clear = read_json_timeout(&mut client).await;
    assert_eq!(clear["t"], "inventory_tree");
    assert!(clear["d"].is_null());
}

#[tokio::test]
async fn webui_subscribe_and_event_arrive_as_remote_events() {
    let (_sink, mut event_rx, addr) = start_server(100).await;
    let (mut client, _) = connect_and_sync(addr, 0).await;

    // Opening a WebUI panel subscribes to its page.
    client
        .send_text(r#"{"t":"webui_subscribe","d":{"page":"creaturebar/main"}}"#)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::WebUiSubscribe { page } = event else {
        panic!("expected WebUiSubscribe, got {event:?}");
    };
    assert_eq!(page, "creaturebar/main");

    // A button tap forwards as a WebUiEvent carrying page/cid/value.
    client
        .send_text(
            r#"{"t":"webui_event","d":{"page":"creaturebar/main","cid":"button:2","value":null}}"#,
        )
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::WebUiEvent { page, cid, value } = event else {
        panic!("expected WebUiEvent, got {event:?}");
    };
    assert_eq!(page, "creaturebar/main");
    assert_eq!(cid, "button:2");
    assert_eq!(value, serde_json::Value::Null);
}

#[tokio::test]
async fn webui_render_broadcasts_the_serialized_tree() {
    use vellum_fe::data::webui::WebUiNode;
    let (mut sink, _event_rx, addr) = start_server(100).await;
    let (mut client, _) = connect_and_sync(addr, 0).await;

    // A page render broadcasts the serialized component tree to phone clients.
    let tree: WebUiNode = serde_json::from_str(
        r#"{ "t": "page", "title": "Creatures", "children": [
            { "t": "button", "cid": "button:2", "label": "Attack", "variant": "danger" }
        ] }"#,
    )
    .unwrap();
    sink.push_webui_render("creaturebar/main".to_string(), 7, tree);

    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "webui_render");
    assert_eq!(delta["d"]["page"], "creaturebar/main");
    assert_eq!(delta["d"]["seq"], 7);
    assert_eq!(delta["d"]["tree"]["t"], "page");
    assert_eq!(delta["d"]["tree"]["children"][0]["label"], "Attack");
    assert_eq!(delta["d"]["tree"]["children"][0]["variant"], "danger");
    // Lean wire: absent optional fields aren't serialized as nulls.
    assert!(delta["d"]["tree"].get("markers").is_none());
}

#[tokio::test]
async fn touch_wheel_get_and_put_arrive_as_addressed_events() {
    let (_sink, mut event_rx, addr) = start_server(100).await;
    let (mut editor, _) = connect_and_sync(addr, 0).await;

    // A get carries the scope through as a TouchWheelGet event.
    editor
        .send_text(r#"{"t":"touch_wheel_get","d":{"request_id":3,"scope":"profile"}}"#)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out waiting for touch_wheel_get")
        .expect("event channel open");
    let RemoteEvent::TouchWheelGet {
        request_id, scope, ..
    } = event
    else {
        panic!("expected TouchWheelGet event, got {event:?}");
    };
    assert_eq!(request_id, 3);
    assert_eq!(scope, "profile");

    // A put carries the slice array through as a TouchWheelPut event.
    editor
        .send_text(
            r#"{"t":"touch_wheel_put","d":{"request_id":4,"scope":"profile","slices":[{"label":"Room","client":"open:room"},{"label":"Look","command":"look"}]}}"#,
        )
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out waiting for touch_wheel_put")
        .expect("event channel open");
    let RemoteEvent::TouchWheelPut {
        request_id,
        scope,
        slices,
        ..
    } = event
    else {
        panic!("expected TouchWheelPut event, got {event:?}");
    };
    assert_eq!(request_id, 4);
    assert_eq!(scope, "profile");
    let arr = slices.as_array().expect("slices is an array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["label"], "Room");
    assert_eq!(arr[0]["client"], "open:room");
    assert_eq!(arr[1]["command"], "look");

    // A malformed put (slices not an array) is rejected client-side and
    // produces no event.
    editor
        .send_text(
            r#"{"t":"touch_wheel_put","d":{"request_id":5,"scope":"profile","slices":"nope"}}"#,
        )
        .await;
    let timed_out = tokio::time::timeout(std::time::Duration::from_millis(300), event_rx.recv())
        .await
        .is_err();
    assert!(
        timed_out,
        "a non-array touch_wheel_put must not emit an event"
    );
}

#[tokio::test]
async fn wheels_flow_definitions_out_picks_in() {
    let (mut sink, mut event_rx, addr) = start_server(100).await;

    let mut wheel_config = vellum_fe::config::Config::default();
    wheel_config.controller_wheel = vec![
        vellum_fe::config::WheelSlice {
            label: "look".into(),
            command: "look".into(),
            ..Default::default()
        },
        vellum_fe::config::WheelSlice {
            label: "stance".into(),
            command: String::new(),
            slices: vec![vellum_fe::config::WheelSlice {
                label: "defensive".into(),
                command: "stance defensive".into(),
                color: Some("#2e8b57".into()),
                ..Default::default()
            }],
            ..Default::default()
        },
    ];
    sink.set_wheels(&wheel_config);

    let mut client = WsClient::connect(addr).await;
    assert_eq!(read_json_timeout(&mut client).await["t"], "hello");
    client.send_resume(0).await;
    assert_eq!(read_json_timeout(&mut client).await["t"], "snapshot");
    assert_eq!(read_json_timeout(&mut client).await["t"], "macros");

    // Definitions arrive after the macros: labels, colors, and folder
    // structure — no commands.
    let wheels = read_json_timeout(&mut client).await;
    assert_eq!(wheels["t"], "wheels");
    let d = &wheels["d"];
    assert_eq!(d["default"][0]["label"], "look");
    assert_eq!(d["default"][1]["slices"][0]["label"], "defensive");
    assert_eq!(d["default"][1]["slices"][0]["color"], "#2e8b57");
    assert!(
        !wheels.to_string().contains("stance defensive"),
        "commands must never reach the client"
    );

    // A pick comes back as a key+path event...
    client
        .send_text(r#"{"t":"wheel_pick","d":{"key":"","path":[1,0]}}"#)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::WheelPick { key, path } = event else {
        panic!("expected WheelPick event");
    };
    assert_eq!((key.as_str(), path.as_slice()), ("", &[1usize, 0][..]));
    // ...which core resolves back to the command.
    assert_eq!(
        wheel_config.wheel_pick_command(&key, &path),
        Some("stance defensive".to_string())
    );

    // A wheel-config change pushes fresh definitions as a delta.
    sink.set_wheels(&vellum_fe::config::Config::default());
    let update = read_json_timeout(&mut client).await;
    assert_eq!(update["t"], "wheels");
    assert_eq!(update["d"]["default"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn wrong_token_is_denied_and_disconnected() {
    let (mut sink, _event_rx, addr) = start_server(100).await;
    sink.push_text("main", styled("secret", "main"));

    let mut client = WsClient::connect_unauthenticated(addr).await;
    client
        .send_text(r#"{"t":"auth","d":{"token":"wrong"}}"#)
        .await;
    let reply = read_json_timeout(&mut client).await;
    assert_eq!(reply["t"], "denied", "wrong token gets denied, not hello");

    // And a non-auth first message is equally denied.
    let mut client = WsClient::connect_unauthenticated(addr).await;
    client.send_text(r#"{"t":"cmd","d":{"text":"look"}}"#).await;
    let reply = read_json_timeout(&mut client).await;
    assert_eq!(reply["t"], "denied");
}

#[tokio::test]
async fn auth_failures_throttle_further_attempts() {
    let (_sink, _event_rx, addr) = start_server(100).await;

    for _ in 0..5 {
        let mut client = WsClient::connect_unauthenticated(addr).await;
        client
            .send_text(r#"{"t":"auth","d":{"token":"wrong"}}"#)
            .await;
        let reply = read_json_timeout(&mut client).await;
        assert_eq!(reply["t"], "denied");
    }

    // Locked out: even the correct token is refused until the window
    // drains.
    let mut client = WsClient::connect_unauthenticated(addr).await;
    client
        .send_text(&format!(r#"{{"t":"auth","d":{{"token":"{TEST_TOKEN}"}}}}"#))
        .await;
    let reply = read_json_timeout(&mut client).await;
    assert_eq!(reply["t"], "denied", "lockout rejects even valid tokens");
}

#[tokio::test]
async fn macro_save_and_delete_arrive_as_events() {
    let (_sink, mut event_rx, addr) = start_server(100).await;
    let (mut client, _) = connect_and_sync(addr, 0).await;

    client
        .send_text(
            r##"{"t":"macro_save","d":{"group":"Couch","label":"Nap","command":"sleep","color":"#d9b44f","confirm":true,"original":{"group":null,"label":"Old nap"}}}"##,
        )
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::MacroSave {
        group,
        label,
        command,
        color,
        confirm,
        insert,
        client: client_action,
        options,
        original,
    } = event
    else {
        panic!("expected MacroSave");
    };
    assert_eq!(group.as_deref(), Some("Couch"));
    assert_eq!(label, "Nap");
    assert_eq!(command, "sleep");
    assert_eq!(color.as_deref(), Some("#d9b44f"));
    assert!(confirm);
    assert!(!insert);
    assert_eq!(client_action, None);
    assert!(options.is_empty());
    assert_eq!(original, Some((None, "Old nap".to_string())));

    // A type-in button: the trailing \r ("type, then send") must survive
    // the trim that protects ordinary commands.
    client
        .send_text(
            r#"{"t":"macro_save","d":{"group":"Words","label":"door","command":"door\r","insert":true}}"#,
        )
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::MacroSave {
        command, insert, ..
    } = event
    else {
        panic!("expected MacroSave");
    };
    assert!(insert);
    assert_eq!(command, "door\r");

    // A menu button: options and no direct command.
    client
        .send_text(
            r#"{"t":"macro_save","d":{"group":"Couch","label":"Travel","command":"","options":[{"label":"Bank","command":";go2 bank"},{"label":"Gate","command":";go2 gate","confirm":true},{"label":"second","command":"second","insert":true}]}}"#,
        )
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::MacroSave {
        label,
        command,
        options,
        ..
    } = event
    else {
        panic!("expected MacroSave");
    };
    assert_eq!(label, "Travel");
    assert!(command.is_empty());
    assert_eq!(options.len(), 3);
    assert_eq!(options[1].command, ";go2 gate");
    assert!(options[1].confirm);
    assert!(options[2].insert, "per-option insert flag forwarded");

    // A client-action button: no command, the action ships instead.
    client
        .send_text(
            r#"{"t":"macro_save","d":{"group":null,"label":"Chars","command":"","client":"shell:characters"}}"#,
        )
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::MacroSave {
        label,
        command,
        client: client_action,
        ..
    } = event
    else {
        panic!("expected MacroSave");
    };
    assert_eq!(label, "Chars");
    assert!(command.is_empty());
    assert_eq!(client_action.as_deref(), Some("shell:characters"));

    // Empty label/command is rejected at parse time, not forwarded.
    client
        .send_text(r#"{"t":"macro_save","d":{"group":null,"label":"  ","command":"x"}}"#)
        .await;
    client
        .send_text(r#"{"t":"macro_delete","d":{"group":null,"label":"Heal"}}"#)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    let RemoteEvent::MacroDelete { group, label } = event else {
        panic!("expected MacroDelete (blank save must not forward)");
    };
    assert_eq!(group, None);
    assert_eq!(label, "Heal");
}

#[tokio::test]
async fn effects_flow_in_snapshot_and_deltas() {
    let (mut sink, _event_rx, addr) = start_server(100).await;

    let mut gs = GameState::new();
    gs.effects.insert(
        "Buffs".to_string(),
        vellum_fe::data::widget::ActiveEffectsContent {
            category: "Buffs".to_string(),
            effects: vec![vellum_fe::data::widget::ActiveEffect {
                id: "509".to_string(),
                text: "Strength of the Bull".to_string(),
                value: 92,
                time: "0:24:10".to_string(),
                expires_at: None,
                bar_color: None,
                text_color: None,
            }],
            generation: 1,
        },
    );
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    // Snapshot carries the effects.
    let (mut client, snapshot) = connect_and_sync(addr, 0).await;
    assert_eq!(snapshot["d"]["effects"][0]["category"], "Buffs");
    assert_eq!(
        snapshot["d"]["effects"][0]["effects"][0]["text"],
        "Strength of the Bull"
    );

    // A change broadcasts an effects delta.
    gs.effects.get_mut("Buffs").unwrap().effects[0].time = "0:20:00".to_string();
    gs.effects.get_mut("Buffs").unwrap().generation += 1;
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));
    let delta = read_json_timeout(&mut client).await;
    assert_eq!(delta["t"], "effects");
    assert_eq!(delta["d"][0]["effects"][0]["time"], "0:20:00");
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

#[tokio::test]
async fn doll_json_requires_pairing_token() {
    let (_sink, _event_rx, addr) = start_server(10).await;
    let response = http_get(addr, "/doll.json").await;
    assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
    let response = http_get(addr, "/doll.json?token=wrong").await;
    assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
}

#[tokio::test]
async fn doll_json_returns_payload_shape() {
    // Whether a skin with doll art is active depends on the machine's
    // config, so assert the contract, not the content: valid JSON with a
    // boolean `base` and object `anchors`/`dots`/`overlays` fields.
    let (_sink, _event_rx, addr) = start_server(10).await;
    let response = http_get(addr, &format!("/doll.json?token={TEST_TOKEN}")).await;
    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    let body = response.split("\r\n\r\n").nth(1).expect("body");
    let payload: serde_json::Value = serde_json::from_str(body).expect("valid JSON");
    assert!(payload["base"].is_boolean());
    assert!(payload["anchors"].is_object());
    assert!(payload["dots"].is_object());
    assert!(payload["overlays"].is_object());
}

#[tokio::test]
async fn doll_image_rejects_bad_requests() {
    let (_sink, _event_rx, addr) = start_server(10).await;
    // No token: forbidden even for nonsense.
    let response = http_get(addr, "/doll/image?kind=base").await;
    assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
    // Unknown kind is never served, active skin or not.
    let response = http_get(addr, &format!("/doll/image?kind=bogus&token={TEST_TOKEN}")).await;
    assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
}

/// Connect as a status-only watcher: send `subscribe {mode:"watch"}` before
/// `resume`, then drain the snapshot and the macros/wheels that follow.
async fn connect_watching(addr: std::net::SocketAddr) -> (WsClient, serde_json::Value) {
    let mut client = WsClient::connect(addr).await;
    let hello = read_json_timeout(&mut client).await;
    assert_eq!(hello["t"], "hello");
    client
        .send_text(r#"{"t":"subscribe","d":{"mode":"watch"}}"#)
        .await;
    client.send_resume(0).await;
    let snapshot = read_json_timeout(&mut client).await;
    assert_eq!(snapshot["t"], "snapshot");
    let macros = read_json_timeout(&mut client).await;
    assert_eq!(macros["t"], "macros");
    let wheels = read_json_timeout(&mut client).await;
    assert_eq!(wheels["t"], "wheels");
    (client, snapshot)
}

#[tokio::test]
async fn watch_client_gets_status_without_scrollback() {
    let (mut sink, _event_rx, addr) = start_server(100).await;

    // Buffered scrollback that a Play client WOULD receive.
    sink.push_text("main", styled("pre-connect line", "main"));

    let (_client, snapshot) = connect_watching(addr).await;

    assert!(
        snapshot["d"].get("text").is_none(),
        "a watcher must not be sent scrollback: {}",
        snapshot["d"]
    );
    // The status a watcher exists to render is all present.
    assert!(snapshot["d"].get("vitals").is_some());
    assert!(snapshot["d"].get("indicators").is_some());
    assert!(snapshot["d"].get("injuries").is_some());
    assert!(snapshot["d"].get("rt").is_some());
}

#[tokio::test]
async fn watch_client_receives_status_deltas_but_no_text() {
    let (mut sink, _event_rx, addr) = start_server(100).await;
    let (mut client, _) = connect_watching(addr).await;

    // Text must be filtered out entirely...
    sink.push_text("main", styled("noise the watcher ignores", "main"));

    // ...so the next frame the watcher sees is the vitals change, not the
    // text line pushed before it. This is the assertion that proves
    // filtering rather than mere reordering.
    let mut gs = GameState::new();
    gs.vitals.health = 42;
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let frame = read_json_timeout(&mut client).await;
    assert_eq!(
        frame["t"], "vitals",
        "expected vitals, got {} -- text should have been filtered",
        frame["t"]
    );
    assert_eq!(frame["d"]["health"], 42);
}

#[tokio::test]
async fn watch_client_receives_group_roster_changes() {
    let (mut sink, _event_rx, addr) = start_server(100).await;
    let (mut client, _) = connect_watching(addr).await;

    let mut gs = GameState::new();
    gs.group.replace(
        vellum_fe::core::group::GroupLeader::SelfLed,
        vec![vellum_fe::core::group::GroupMember {
            id: "-1".to_string(),
            noun: "bob".to_string(),
            name: "Bob".to_string(),
        }],
    );
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let frame = read_json_timeout(&mut client).await;
    assert_eq!(frame["t"], "group", "got {}", frame["t"]);
    assert_eq!(frame["d"]["members"][0]["name"], "Bob");
    assert_eq!(frame["d"]["confirmed"], true);
}

#[tokio::test]
async fn a_watcher_and_a_player_share_one_server() {
    // The multi-account case: a watcher connected alongside the phone must
    // not change what the phone receives.
    let (mut sink, _event_rx, addr) = start_server(100).await;

    let (mut player, player_snapshot) = connect_and_sync(addr, 0).await;
    let (mut watcher, watch_snapshot) = connect_watching(addr).await;

    // The player's snapshot carries scrollback machinery; the watcher's does
    // not, from the same server at the same moment.
    assert!(player_snapshot["d"].get("map_state").is_some());
    assert!(watch_snapshot["d"].get("text").is_none());

    sink.push_text("main", styled("only the player sees this", "main"));
    let frame = read_json_timeout(&mut player).await;
    assert_eq!(frame["t"], "text");
    assert_eq!(
        frame["d"]["line"]["segments"][0]["text"],
        "only the player sees this"
    );

    // Both see a status change.
    let mut gs = GameState::new();
    gs.vitals.mana = 7;
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let watcher_frame = read_json_timeout(&mut watcher).await;
    assert_eq!(watcher_frame["t"], "vitals");
    assert_eq!(watcher_frame["d"]["mana"], 7);

    let player_frame = read_json_timeout(&mut player).await;
    assert_eq!(player_frame["t"], "vitals");
    assert_eq!(player_frame["d"]["mana"], 7);
}

// ==================== Multi-account hub (end-to-end) ====================

/// The hub's frame appliers, driven by a REAL server rather than by
/// hand-written JSON. The unit tests assert what the hub does with a frame;
/// this asserts the frames it actually receives match that shape.
#[tokio::test]
async fn hub_applies_real_server_frames_to_a_peer() {
    use vellum_fe::core::multiaccount::PeerStatus;

    let (mut sink, _event_rx, addr) = start_server(100).await;

    // Push state the way the app does, then connect as a watcher and feed
    // every frame through the hub's applier.
    let mut gs = GameState::new();
    gs.vitals.health = 63;
    gs.status.set("IconSTUNNED", true);
    gs.injuries.insert("head".to_string(), 2);
    gs.stance.update(80, "defensive (80%)");
    gs.gs4_experience
        .update_mind_state(42, "muddled".to_string());
    gs.encumbrance.update_level(17, "Light".to_string());
    gs.group.replace(
        vellum_fe::core::group::GroupLeader::SelfLed,
        vec![vellum_fe::core::group::GroupMember {
            id: "-1".to_string(),
            noun: "bob".to_string(),
            name: "Bob".to_string(),
        }],
    );
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let (_client, snapshot) = connect_watching(addr).await;

    let mut peer = PeerStatus {
        character: "Alice".to_string(),
        port: 8040,
        ..Default::default()
    };
    vellum_fe::core::multiaccount::hub::apply_frame_for_test(&mut peer, &snapshot);

    // Every field the display renders, sourced from a real snapshot.
    assert_eq!(peer.vitals.health, 63);
    assert!(peer.indicators.stunned(), "indicators survived the wire");
    assert_eq!(peer.injuries.get("head"), Some(&2));
    assert!(peer.group.leads());
    assert_eq!(peer.group.members[0].name, "Bob");
    assert_eq!(
        peer.stance.as_ref().map(|g| g.value),
        Some(80),
        "numeric stance, not a re-parsed display string"
    );
    assert_eq!(
        peer.stance.as_ref().map(|g| g.text.as_str()),
        Some("defensive")
    );
    assert_eq!(peer.mind.as_ref().map(|g| g.value), Some(42));
    assert_eq!(peer.encumbrance.as_ref().map(|g| g.value), Some(17));
    assert!(peer.connected);
}

#[tokio::test]
async fn hub_tracks_live_status_deltas_from_a_real_server() {
    use vellum_fe::core::multiaccount::PeerStatus;

    let (mut sink, _event_rx, addr) = start_server(100).await;
    let (mut client, _) = connect_watching(addr).await;

    let mut peer = PeerStatus::default();

    let mut gs = GameState::new();
    gs.vitals.health = 12;
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));
    let frame = read_json_timeout(&mut client).await;
    vellum_fe::core::multiaccount::hub::apply_frame_for_test(&mut peer, &frame);
    assert_eq!(peer.vitals.health, 12);

    // A roster change arrives as its own delta and replaces the roster.
    gs.group.replace(
        vellum_fe::core::group::GroupLeader::Other(vellum_fe::core::group::GroupMember {
            id: "-9".to_string(),
            noun: "zed".to_string(),
            name: "Zed".to_string(),
        }),
        vec![],
    );
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));
    let frame = read_json_timeout(&mut client).await;
    vellum_fe::core::multiaccount::hub::apply_frame_for_test(&mut peer, &frame);
    assert!(!peer.group.leads(), "now following Zed");
    assert_eq!(peer.vitals.health, 12, "unrelated state must persist");
}

#[tokio::test]
async fn hub_receives_effects_hands_and_absolute_vitals() {
    use vellum_fe::core::multiaccount::PeerStatus;

    let (mut sink, _event_rx, addr) = start_server(100).await;

    let mut gs = GameState::new();
    gs.left_hand = Some("a reinforced shield".to_string());
    gs.right_hand = Some("a longsword".to_string());
    gs.spell = Some("Spirit Warding I".to_string());
    gs.minivitals
        .update_vital("health", 51, 51, "health 51/51".to_string());
    gs.minivitals
        .update_vital("mana", 32, 64, "mana 32/64".to_string());
    gs.effects.insert(
        "Cooldowns".to_string(),
        vellum_fe::data::ActiveEffectsContent {
            category: "Cooldowns".to_string(),
            effects: vec![vellum_fe::data::ActiveEffect {
                id: "1".to_string(),
                text: "Berserk".to_string(),
                value: 50,
                time: "00:01:00".to_string(),
                expires_at: Some(1_760),
                bar_color: None,
                text_color: None,
            }],
            generation: 1,
        },
    );
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let (_client, snapshot) = connect_watching(addr).await;

    let mut peer = PeerStatus::default();
    vellum_fe::core::multiaccount::hub::apply_frame_for_test(&mut peer, &snapshot);

    // Absolute vitals: the "51/51" a percentage cannot express.
    assert_eq!(peer.minivitals.get("health"), Some(&(51, 51)));
    assert_eq!(peer.minivitals.get("mana"), Some(&(32, 64)));
    // Stamina/spirit never reported: absent, not 0/0, which would render as
    // a dead character.
    assert!(peer.minivitals.get("stamina").is_none());

    assert_eq!(peer.left_hand.as_deref(), Some("a reinforced shield"));
    assert_eq!(peer.right_hand.as_deref(), Some("a longsword"));
    assert_eq!(peer.prepared_spell.as_deref(), Some("Spirit Warding I"));

    let cooldowns = peer.effects.get("Cooldowns").expect("cooldowns category");
    assert_eq!(cooldowns.effects[0].text, "Berserk");
    assert_eq!(
        cooldowns.effects[0].expires_at,
        Some(1_760),
        "absolute expiry is what lets the card count down locally"
    );
}

#[tokio::test]
async fn hub_receives_field_exp_and_joined_indicator() {
    use vellum_fe::core::multiaccount::PeerStatus;

    let (mut sink, _event_rx, addr) = start_server(100).await;

    let mut gs = GameState::new();
    gs.gs4_experience.field_exp = Some(1_200);
    gs.gs4_experience.max_field_exp = Some(1_500);
    gs.status.set("IconJOINED", true);
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let (_client, snapshot) = connect_watching(addr).await;
    let mut peer = PeerStatus::default();
    vellum_fe::core::multiaccount::hub::apply_frame_for_test(&mut peer, &snapshot);

    assert_eq!(peer.field_exp, Some((1_200, 1_500)));
    assert!(
        peer.indicators.joined(),
        "JOINED must survive the wire -- it is the game's own 'is grouped'"
    );
}

#[tokio::test]
async fn field_exp_is_absent_until_both_halves_are_known() {
    use vellum_fe::core::multiaccount::PeerStatus;

    let (mut sink, _event_rx, addr) = start_server(100).await;

    // A value with no cap cannot be drawn as a bar, so it must not ship as a
    // half-populated gauge that renders at a made-up ratio.
    let mut gs = GameState::new();
    gs.gs4_experience.field_exp = Some(900);
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let (_client, snapshot) = connect_watching(addr).await;
    let mut peer = PeerStatus::default();
    vellum_fe::core::multiaccount::hub::apply_frame_for_test(&mut peer, &snapshot);

    assert_eq!(peer.field_exp, None);
}

#[tokio::test]
async fn watchers_receive_room_changes_without_the_prose() {
    use vellum_fe::core::multiaccount::PeerStatus;

    let (mut sink, _event_rx, addr) = start_server(100).await;
    let (mut client, _) = connect_watching(addr).await;

    // A room change after connect. Before the fix, Room was not in the watch
    // whitelist at all, so a watcher's room froze at whatever the connect
    // snapshot held -- or stayed empty forever if the peer had not logged in
    // yet. That is why the card's room number never appeared.
    let mut gs = GameState::new();
    gs.room_name = Some("Town Square".to_string());
    gs.room_id = Some("12345".to_string());
    gs.exits = vec!["north".to_string()];
    gs.room_description = vec![StyledLine {
        segments: vec![TextSegment::plain("A wide plaza bustles with traffic.")],
        stream: "main".to_string(),
        timestamp: None,
    }];
    sink.flush_state(vellum_fe::core::remote::RemoteStateSnapshot::from_game_state(&gs, &[]));

    let frame = read_json_timeout(&mut client).await;
    assert_eq!(frame["t"], "room", "got {}", frame["t"]);
    assert_eq!(frame["d"]["id"], "12345");
    assert_eq!(frame["d"]["name"], "Town Square");
    // The prose is the bulk; a watcher must not pay for it.
    assert!(
        frame["d"].get("description").is_none()
            || frame["d"]["description"]
                .as_array()
                .is_some_and(|a| a.is_empty()),
        "prose must be stripped for watchers: {}",
        frame["d"]
    );

    // And the hub applies it.
    let mut peer = PeerStatus::default();
    vellum_fe::core::multiaccount::hub::apply_frame_for_test(&mut peer, &frame);
    assert_eq!(peer.room_id.as_deref(), Some("12345"));
    assert_eq!(peer.room_name.as_deref(), Some("Town Square"));
}
