//! Lich WebUI bridge - native WebSocket client.
//!
//! Connects to the per-session Lich WebUI server discovered via the
//! `;ui handshake` -> `<LichWebUI .../>` exchange (see `data::webui`), and
//! relays envelopes between the server and the frontend over channels:
//!
//! ```text
//! Lich (ws://<host>:<port>/ws — host from the handshake url: loopback
//!       for a local Lich, a LAN address for a containerized one)
//!        │  ▲
//!  hello/pages/render/close        subscribe/unsubscribe/event
//!        ▼  │
//!   client task (this module, reconnects with backoff)
//!        │  ▲
//!   WebUiEvent (mpsc)          WebUiClientMessage (mpsc)
//!        ▼  │
//!   frontend (GUI pump)
//! ```
//!
//! Auth: the server accepts the upgrade when the request carries the
//! `lich_webui=<token>` cookie and an allowlisted Origin (the dialed
//! host:port must be in the server's allowed hosts). The token is
//! script-level power (WebUI callbacks run Ruby inside Lich) - it is never
//! logged here and must not appear in errors surfaced to windows.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

use crate::data::webui::{WebUiClientMessage, WebUiServerMessage};

/// Events the client task emits toward the frontend.
#[derive(Clone, Debug)]
pub enum WebUiEvent {
    /// Connection established; carries the server's hello payload.
    Hello {
        schema_version: u32,
        session: crate::data::webui::WebUiSession,
        pages: Vec<crate::data::webui::WebUiPageDescriptor>,
    },
    /// Page registry changed (script started/stopped a page).
    Pages(Vec<crate::data::webui::WebUiPageDescriptor>),
    /// Full component tree push for one subscribed page.
    Render {
        page: String,
        seq: u64,
        tree: crate::data::webui::WebUiNode,
    },
    /// Page is finished (script exited); panel lifecycle is ours.
    PageClosed { page: String },
    /// User-facing notice from the server ("info" | "warn" | "error").
    Notice { level: String, text: String },
    /// Socket dropped; the task keeps retrying until `gave_up`.
    Disconnected { gave_up: bool },
    /// Result of a `fetch_image` request (raw encoded bytes on success).
    ImageFetched {
        src: String,
        data: Result<Vec<u8>, String>,
    },
}

/// Handle to a running bridge task. Dropping the sender ends the task after
/// the current socket closes.
pub struct WebUiHandle {
    pub port: u16,
    outbound_tx: mpsc::UnboundedSender<WebUiClientMessage>,
    task: tokio::task::JoinHandle<()>,
}

impl WebUiHandle {
    pub fn send(&self, message: WebUiClientMessage) {
        let _ = self.outbound_tx.send(message);
    }

    pub fn subscribe(&self, page: &str) {
        self.send(WebUiClientMessage::Subscribe { page: page.to_string() });
    }

    pub fn shutdown(&self) {
        self.task.abort();
    }
}

impl Drop for WebUiHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Spawns the bridge client on the given runtime handle. Events flow out on
/// `event_tx`; client messages are queued via the returned handle.
///
/// The task reconnects with backoff on socket loss and replays every active
/// subscription after each reconnect, so the frontend only re-learns state
/// through the fresh `Hello`/`Render` pushes. A Lich restart changes port
/// and token, so after `MAX_RECONNECT_ATTEMPTS` failures it emits
/// `Disconnected { gave_up: true }` and exits (the user re-runs `.webui`).
pub fn start(
    runtime: &tokio::runtime::Handle,
    host: String,
    port: u16,
    token: String,
    event_tx: mpsc::UnboundedSender<WebUiEvent>,
) -> WebUiHandle {
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
    let task = runtime.spawn(run_bridge(host, port, token, event_tx, outbound_rx));
    WebUiHandle {
        port,
        outbound_tx,
        task,
    }
}

const MAX_RECONNECT_ATTEMPTS: u32 = 8;

async fn run_bridge(
    host: String,
    port: u16,
    token: String,
    event_tx: mpsc::UnboundedSender<WebUiEvent>,
    mut outbound_rx: mpsc::UnboundedReceiver<WebUiClientMessage>,
) {
    // Subscriptions observed on the outbound channel, replayed on reconnect.
    let mut subscriptions: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut attempts: u32 = 0;

    loop {
        match connect(&host, port, &token).await {
            Ok(mut socket) => {
                attempts = 0;
                tracing::info!("WebUI bridge connected to {}:{}", host, port);

                // Replay subscriptions lost with the previous socket.
                for page in &subscriptions {
                    let msg = WebUiClientMessage::Subscribe { page: page.clone() };
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let _ = socket.send(Message::Text(json.into())).await;
                    }
                }

                loop {
                    tokio::select! {
                        outbound = outbound_rx.recv() => {
                            let Some(message) = outbound else {
                                // Frontend dropped the handle: shut down.
                                let _ = socket.close(None).await;
                                return;
                            };
                            match &message {
                                WebUiClientMessage::Subscribe { page } => {
                                    subscriptions.insert(page.clone());
                                }
                                WebUiClientMessage::Unsubscribe { page } => {
                                    subscriptions.remove(page);
                                }
                                WebUiClientMessage::Event { .. } => {}
                            }
                            match serde_json::to_string(&message) {
                                Ok(json) => {
                                    if let Err(err) = socket.send(Message::Text(json.into())).await {
                                        tracing::warn!("WebUI bridge send failed: {}", err);
                                        break;
                                    }
                                }
                                Err(err) => {
                                    tracing::error!("WebUI bridge failed to encode message: {}", err);
                                }
                            }
                        }
                        incoming = socket.next() => {
                            match incoming {
                                Some(Ok(Message::Text(raw))) => {
                                    handle_server_text(raw.as_str(), &event_tx);
                                }
                                Some(Ok(Message::Ping(payload))) => {
                                    let _ = socket.send(Message::Pong(payload)).await;
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    tracing::info!("WebUI bridge socket closed by server");
                                    break;
                                }
                                Some(Ok(_)) => {} // binary/pong: ignore
                                Some(Err(err)) => {
                                    tracing::warn!("WebUI bridge read error: {}", err);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Err(err) => {
                attempts += 1;
                tracing::warn!(
                    "WebUI bridge connect to {}:{} failed (attempt {}/{}): {}",
                    host,
                    port,
                    attempts,
                    MAX_RECONNECT_ATTEMPTS,
                    err
                );
                if attempts >= MAX_RECONNECT_ATTEMPTS {
                    let _ = event_tx.send(WebUiEvent::Disconnected { gave_up: true });
                    return;
                }
            }
        }

        if event_tx
            .send(WebUiEvent::Disconnected { gave_up: false })
            .is_err()
        {
            return; // frontend gone
        }

        // Exponential backoff, capped: 500ms, 1s, 2s, 4s, 8s, 8s, ...
        let delay = std::time::Duration::from_millis(500u64 << attempts.min(4));
        tokio::time::sleep(delay).await;
    }
}

/// Fetches one image from the WebUI server's `/files/` route on a background
/// task; the result arrives as `WebUiEvent::ImageFetched` on `event_tx`.
///
/// The route requires the same auth cookie as the WebSocket. Responses are
/// always `Connection: close` (one request per connection, server.rb), so
/// the body is simply everything after the headers until EOF.
pub fn fetch_image(
    runtime: &tokio::runtime::Handle,
    host: String,
    port: u16,
    token: String,
    src: String,
    event_tx: mpsc::UnboundedSender<WebUiEvent>,
) {
    runtime.spawn(async move {
        let fetch = http_get_files(&host, port, &token, &src);
        let data = match tokio::time::timeout(std::time::Duration::from_secs(10), fetch).await {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(err)) => Err(err.to_string()),
            Err(_) => Err("image fetch timed out".to_string()),
        };
        let _ = event_tx.send(WebUiEvent::ImageFetched { src, data });
    });
}

async fn http_get_files(host: &str, port: u16, token: &str, path: &str) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        path.starts_with("/files/") && !path.contains("..") && !path.contains(['\r', '\n']),
        "unsupported image source"
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect((host, port)).await?;
    let encoded_path = path.replace(' ', "%20");
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}:{}\r\nCookie: lich_webui={}\r\nConnection: close\r\n\r\n",
        encoded_path, host, port, token
    );
    stream.write_all(request.as_bytes()).await?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;

    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("malformed HTTP response"))?;
    let head = String::from_utf8_lossy(&response[..header_end]);
    let status_line = head.lines().next().unwrap_or_default();
    anyhow::ensure!(
        status_line.split_whitespace().nth(1) == Some("200"),
        "image fetch failed: {}",
        status_line
    );
    Ok(response[header_end + 4..].to_vec())
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Opens the authenticated WebSocket. The server requires the auth cookie
/// AND an allowlisted Origin (`http://<host>:<port>` — the dialed address,
/// which must be in the server's allowed hosts) on the upgrade; the cookie
/// value is the token itself, so no /auth round-trip is needed.
async fn connect(host: &str, port: u16, token: &str) -> anyhow::Result<WsStream> {
    let url = format!("ws://{}:{}/ws", host, port);
    let mut request = url.into_client_request()?;
    let headers = request.headers_mut();
    headers.insert(
        "Origin",
        format!("http://{}:{}", host, port).parse()?,
    );
    headers.insert("Cookie", format!("lich_webui={}", token).parse()?);

    let (socket, _response) = tokio_tungstenite::connect_async(request).await?;
    Ok(socket)
}

fn handle_server_text(raw: &str, event_tx: &mpsc::UnboundedSender<WebUiEvent>) {
    let message: WebUiServerMessage = match serde_json::from_str(raw) {
        Ok(message) => message,
        Err(err) => {
            tracing::warn!("WebUI bridge: unparseable envelope ({})", err);
            return;
        }
    };

    let event = match message {
        WebUiServerMessage::Hello {
            schema_version,
            session,
            pages,
            ..
        } => {
            if schema_version != 1 {
                tracing::warn!(
                    "WebUI bridge: server schema {} (client built for 1); continuing best-effort",
                    schema_version
                );
            }
            WebUiEvent::Hello {
                schema_version,
                session,
                pages,
            }
        }
        WebUiServerMessage::Pages { pages } => WebUiEvent::Pages(pages),
        WebUiServerMessage::Render { page, seq, tree } => WebUiEvent::Render { page, seq, tree },
        WebUiServerMessage::Close { page } => WebUiEvent::PageClosed { page },
        WebUiServerMessage::Notice { level, text } => WebUiEvent::Notice { level, text },
        WebUiServerMessage::Notify { title, text } => WebUiEvent::Notice {
            level: "info".to_string(),
            text: match title {
                Some(title) => format!("{}: {}", title, text),
                None => text,
            },
        },
        WebUiServerMessage::Unknown => return,
    };
    let _ = event_tx.send(event);
}
