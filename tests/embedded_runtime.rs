//! Startup-contract tests for the embedded mobile bootstrap
//! (`frontend::headless::embedded`): the `(port, token)` a shell receives
//! must be the web server's actual listening endpoint, failures must reach
//! the caller, and concurrent starts must be idempotent.
//!
//! The embedded runtime owns process-global state (the `CORE` singleton,
//! `VELLUM_FE_DIR`, the session registry's cached directory), so every
//! scenario runs in its own child process pointed at a temporary data
//! directory — the same fixture-host pattern `tests/web_server.rs` uses.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;

const FIXTURE_MODE_ENV: &str = "EMBEDDED_FIXTURE_MODE";
const FIXTURE_DATA_DIR_ENV: &str = "EMBEDDED_FIXTURE_DATA_DIR";
const FIXTURE_PORT_ENV: &str = "EMBEDDED_FIXTURE_PORT";

/// Reserve an ephemeral port with room to walk upward from it.
fn reserve_walkable_port() -> std::net::TcpListener {
    loop {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("reserve ephemeral port");
        if listener.local_addr().expect("reservation address").port() <= u16::MAX - 20 {
            return listener;
        }
    }
}

fn run_fixture(mode: &str, pinned: bool) {
    let reservation = reserve_walkable_port();
    let base_port = reservation.local_addr().expect("reservation address").port();
    drop(reservation);

    let data_dir = tempfile::tempdir().expect("temporary data directory");
    // The global settings file (`{VELLUM_FE_DIR}/global/config.toml`) is
    // where the [web] port/pin configuration lives (config/paths.rs).
    let global_dir = data_dir.path().join("global");
    std::fs::create_dir_all(&global_dir).expect("create global config dir");
    std::fs::write(
        global_dir.join("config.toml"),
        format!("[web]\nenabled = true\nport = {base_port}\npinned = {pinned}\n"),
    )
    .expect("write fixture config");

    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("embedded_fixture_host")
        .arg("--nocapture")
        .env(FIXTURE_MODE_ENV, mode)
        .env(FIXTURE_DATA_DIR_ENV, data_dir.path())
        .env(FIXTURE_PORT_ENV, base_port.to_string())
        .env("VELLUM_FE_DIR", data_dir.path())
        .env("VELLUM_FE_RUNTIME_DIR", data_dir.path().join("runtime"))
        .output()
        .expect("spawn embedded fixture");

    assert!(
        output.status.success(),
        "embedded fixture '{mode}' failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Occupying the unpinned base port must make the embedded start report the
/// actual alternate port, and concurrent starts must return one identical
/// instance whose token authenticates.
#[test]
fn embedded_start_walks_occupied_base_port_and_serializes_concurrent_starts() {
    run_fixture("walk_concurrent", false);
}

/// A pinned-port conflict must surface a useful startup error to the shell,
/// leave nothing cached as running, and allow a clean retry once the port
/// frees up.
#[test]
fn embedded_pinned_conflict_fails_start_then_retry_succeeds() {
    run_fixture("pinned_retry", true);
}

/// Hidden child-process entry point. The ordinary test run skips it; the
/// parents above start this same executable with the fixture env set.
#[test]
#[ignore]
fn embedded_fixture_host() {
    let Ok(mode) = std::env::var(FIXTURE_MODE_ENV) else {
        return;
    };
    let data_dir = std::env::var(FIXTURE_DATA_DIR_ENV).expect("fixture data dir");
    let base_port: u16 = std::env::var(FIXTURE_PORT_ENV)
        .expect("fixture port")
        .parse()
        .expect("numeric fixture port");

    match mode.as_str() {
        "walk_concurrent" => fixture_walk_concurrent(&data_dir, base_port),
        "pinned_retry" => fixture_pinned_retry(&data_dir, base_port),
        other => panic!("unknown fixture mode {other}"),
    }
}

fn fixture_walk_concurrent(data_dir: &str, base_port: u16) {
    use vellum_fe::frontend::headless::embedded;

    // Hold the configured base port so the unpinned server must walk.
    let _holder = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, base_port))
        .expect("occupy base port");

    // Concurrent starts: the Android activity and service race this call.
    // Both must receive the same successful instance.
    let first = {
        let dir = data_dir.to_string();
        std::thread::spawn(move || embedded::start(&dir))
    };
    let second = {
        let dir = data_dir.to_string();
        std::thread::spawn(move || embedded::start(&dir))
    };
    let first = first.join().expect("first start thread").expect("first start");
    let second = second
        .join()
        .expect("second start thread")
        .expect("second start");
    assert_eq!(
        first, second,
        "concurrent starts must return identical connection information"
    );

    let (port, token) = first;
    assert_ne!(
        port, base_port,
        "an occupied unpinned base port must be walked, not reported verbatim"
    );
    assert!(
        (base_port..=base_port + 20).contains(&port),
        "walked port {port} must stay in the walk range above {base_port}"
    );

    let status: serde_json::Value =
        serde_json::from_str(&embedded::status_json()).expect("status JSON");
    assert_eq!(status["running"], true);
    assert_eq!(status["port"], port);

    // Final reachability the way the shells do it, on the returned port.
    let health = http_get(port, "/health");
    assert!(health.contains("200"), "health on returned port: {health}");

    // The returned token must actually authenticate the returned endpoint —
    // a server merely answering /health is not proof of a coherent pairing.
    let hello = ws_auth_hello(port, &token);
    assert_eq!(hello["t"], "hello", "returned token must authenticate: {hello}");

    embedded::stop();
    assert!(
        TcpStream::connect(("127.0.0.1", port)).is_err(),
        "stopped instance must release its port"
    );
    let status: serde_json::Value =
        serde_json::from_str(&embedded::status_json()).expect("status JSON");
    assert_eq!(status["running"], false);
}

fn fixture_pinned_retry(data_dir: &str, base_port: u16) {
    use vellum_fe::frontend::headless::embedded;

    let holder = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, base_port))
        .expect("occupy pinned port");

    let error = embedded::start(data_dir).expect_err("pinned conflict must fail startup");
    assert!(
        error.contains("pinned") && error.contains(&base_port.to_string()),
        "pinned-port conflict must name the problem: {error}"
    );
    let status: serde_json::Value =
        serde_json::from_str(&embedded::status_json()).expect("status JSON");
    assert_eq!(
        status["running"], false,
        "a failed startup must never be cached as running"
    );

    // Retry after the conflict clears: the failed attempt's cleanup must not
    // poison the singleton.
    drop(holder);
    let (port, token) = embedded::start(data_dir).expect("retry after freed port");
    assert_eq!(port, base_port, "pinned instance binds exactly its port");

    let health = http_get(port, "/health");
    assert!(health.contains("200"), "health after retry: {health}");
    let hello = ws_auth_hello(port, &token);
    assert_eq!(hello["t"], "hello", "retry token must authenticate: {hello}");

    embedded::stop();
}

fn http_get(port: u16, path: &str) -> String {
    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("connect for HTTP GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .expect("send HTTP GET");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read HTTP response");
    String::from_utf8_lossy(&buf).into_owned()
}

/// Minimal blocking WebSocket client: handshake, send the masked auth frame
/// with `token`, and return the first server text frame (the `hello` that
/// only an authenticated client receives).
fn ws_auth_hello(port: u16, token: &str) -> serde_json::Value {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect ws");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
        .write_all(
            b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n\
              Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
              Sec-WebSocket-Version: 13\r\n\r\n",
        )
        .expect("ws handshake request");

    let mut headers = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).expect("handshake read");
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

    // Masked client text frame carrying the auth message.
    let payload = format!(r#"{{"t":"auth","d":{{"token":"{token}"}}}}"#);
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
    stream.write_all(&frame).expect("send auth frame");

    // Read one unmasked server text frame.
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).expect("frame header");
    assert_eq!(header[0] & 0x0f, 0x1, "expected a text frame");
    let len = match header[1] & 0x7f {
        126 => {
            let mut ext = [0u8; 2];
            stream.read_exact(&mut ext).expect("extended length");
            u16::from_be_bytes(ext) as usize
        }
        127 => {
            let mut ext = [0u8; 8];
            stream.read_exact(&mut ext).expect("extended length");
            u64::from_be_bytes(ext) as usize
        }
        n => n as usize,
    };
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).expect("frame payload");
    serde_json::from_slice(&payload).expect("frame payload is JSON")
}
