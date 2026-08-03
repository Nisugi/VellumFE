//! Lich WebUI bridge ownership, in core.
//!
//! Historically the GUI owned the WebUI socket. Core owns it now so BOTH the
//! desktop GUI and the phone render the same component trees from one bridge.
//! The frontend supplies its tokio `Handle` (core is runtime-agnostic); core
//! then:
//!   - starts the bridge on a parsed `;ui handshake` reply,
//!   - pumps `WebUiEvent`s each tick, fanning each out to (a) the phone as
//!     `RemoteDelta::WebUi*` and (b) a GUI re-emit channel for GUI-side
//!     handling (textures, window kinds) the GUI still owns,
//!   - forwards phone interactions back to Lich as `WebUiClientMessage`.
//!
//! Image (`/files/`) fetches stay a frontend concern for the GUI (it needs
//! GUI image textures); the phone fetches images over plain HTTP itself. Core just
//! exposes the endpoint.

use super::AppCore;
use crate::data::webui::{WebUiClientMessage, WebUiHandshake};
use crate::webui::WebUiEvent;

impl AppCore {
    /// Install the GUI re-emit channel. The GUI calls this once at startup so
    /// core-pumped bridge events reach the GUI-side handling. Headless/TUI
    /// leave it None (no local WebUI renderer).
    pub fn set_webui_gui_channel(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<WebUiEvent>,
    ) {
        self.webui_gui_tx = Some(tx);
    }

    /// Whether Lich WebUI is reachable this session (Lich-attached only). The
    /// remote session info advertises this so the phone shows the affordance
    /// only when it will work.
    pub fn webui_available(&self) -> bool {
        self.webui_available
    }

    /// Mark whether this session can reach Lich WebUI. Set false for direct
    /// eAccess (no Lich), true for a Lich-proxied session. Frontends call
    /// this once the connection mode is known.
    pub fn set_webui_available(&mut self, available: bool) {
        self.webui_available = available;
        // Advertise to phone clients so they show the WebUI affordance only
        // when Lich is attached.
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_webui_available(available);
        }
    }

    /// Trigger the WebUI handshake (`;ui handshake`) once per session. The
    /// reply arrives on the game stream and is captured into the message
    /// processor; `take_webui_handshake` + `start_webui` complete the setup.
    /// No-op when WebUI is unavailable (direct connection) or already sent.
    pub fn request_webui_handshake(&mut self) -> bool {
        if !self.webui_available || self.webui_handshake_sent {
            return false;
        }
        self.webui_handshake_sent = true;
        // The `;ui handshake` line goes to Lich on the game socket; the reply
        // is one <LichWebUI .../> line parsed into pending_webui_handshake.
        // AppCore has no game socket (runtime lives in the frontend), so queue
        // it for the frontend to send raw over its command channel.
        self.webui_pending_raw.push(";ui handshake".to_string());
        true
    }

    /// Drain raw game commands core queued for the frontend to send (today:
    /// the WebUI `;ui handshake`). The frontend sends each over its game
    /// command channel once per tick.
    pub fn take_webui_pending_raw(&mut self) -> Vec<String> {
        std::mem::take(&mut self.webui_pending_raw)
    }

    /// Take a parsed handshake reply captured by the message processor, if
    /// any. The frontend calls this each tick and, on Some, calls
    /// `start_webui` with its runtime handle.
    pub fn take_webui_handshake(&mut self) -> Option<WebUiHandshake> {
        self.message_processor.pending_webui_handshake.take()
    }

    /// Start (or restart) the bridge from a handshake reply, using the
    /// frontend's tokio handle. Idempotent-ish: an existing bridge is dropped
    /// first (its task aborts on Drop).
    pub fn start_webui(
        &mut self,
        runtime: &tokio::runtime::Handle,
        handshake: &WebUiHandshake,
    ) {
        let (host, port) = handshake.endpoint();
        let Some(token) = handshake.token() else {
            self.add_system_message("Lich WebUI handshake carried no auth token.");
            return;
        };
        let token = token.to_string();

        // Fresh event channel; drop any prior bridge first.
        self.webui_bridge = None;
        self.webui_rx = None;
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<WebUiEvent>();
        self.webui_event_tx = Some(event_tx.clone());
        self.webui_endpoint = Some((host.clone(), port, token.clone()));
        let handle = crate::webui::start(runtime, host.clone(), port, token, event_tx);
        self.webui_bridge = Some(handle);
        self.webui_rx = Some(event_rx);
        tracing::info!("WebUI bridge connecting to {}:{}", host, port);
    }

    /// (host, port, token) for `/files/` image fetches; the GUI reads this to
    /// fetch textures over the bridge's HTTP endpoint.
    pub fn webui_endpoint(&self) -> Option<&(String, u16, String)> {
        self.webui_endpoint.as_ref()
    }

    /// The sender the GUI clones into `fetch_image` so results return on the
    /// channel `pump_webui` drains (image results also reach the GUI re-emit).
    pub fn webui_event_sender(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<WebUiEvent>> {
        self.webui_event_tx.clone()
    }

    /// Subscribe to a page on the bridge (a phone opening a panel, or the
    /// GUI's own panels). Remembered so it replays when a fresh socket's
    /// Hello arrives (the bridge may not be up yet on the first subscribe).
    pub fn webui_subscribe(&mut self, page: &str) {
        self.webui_subscribed.insert(page.to_string());
        if let Some(bridge) = &self.webui_bridge {
            bridge.subscribe(page);
        }
    }

    pub fn webui_unsubscribe(&mut self, page: &str) {
        self.webui_subscribed.remove(page);
        if let Some(bridge) = &self.webui_bridge {
            bridge.send(WebUiClientMessage::Unsubscribe { page: page.to_string() });
        }
    }

    /// Forward a component interaction to Lich (a button click, input submit,
    /// row click — from either frontend).
    pub fn webui_send_event(&self, page: String, cid: String, value: serde_json::Value) {
        if let Some(bridge) = &self.webui_bridge {
            bridge.send(WebUiClientMessage::Event { page, cid, value });
        }
    }

    /// Drain bridge events this tick, fanning each out to the phone (as
    /// `RemoteDelta::WebUi*`) and to the GUI re-emit channel. Called by the
    /// frontend once per tick/frame. Returns true if any event was processed
    /// (so the GUI can request a repaint).
    pub fn pump_webui(&mut self) -> bool {
        let Some(rx) = self.webui_rx.as_mut() else {
            return false;
        };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        if events.is_empty() {
            return false;
        }
        for event in &events {
            // (a) Relay to the phone as a broadcast delta.
            self.relay_webui_to_phone(event);
            // (b) Re-emit to the GUI for its GUI-side handling.
            if let Some(tx) = &self.webui_gui_tx {
                let _ = tx.send(event.clone());
            }
        }
        true
    }

    /// Fan one bridge event out to phone clients via the remote sink.
    fn relay_webui_to_phone(&mut self, event: &WebUiEvent) {
        let Some(remote) = self.message_processor.remote.as_mut() else {
            return;
        };
        match event {
            WebUiEvent::Hello { pages, .. } => {
                self.webui_pages = pages.clone();
                remote.push_webui_pages(pages.clone());
                remote.push_webui_connected(true);
                // A fresh socket has no subscriptions; replay ours so renders
                // resume for every page a client still has open.
                if let Some(bridge) = &self.webui_bridge {
                    for page in &self.webui_subscribed {
                        bridge.subscribe(page);
                    }
                }
            }
            WebUiEvent::Pages(pages) => {
                self.webui_pages = pages.clone();
                remote.push_webui_pages(pages.clone());
            }
            WebUiEvent::Render { page, seq, tree } => {
                remote.push_webui_render(page.clone(), *seq, tree.clone());
            }
            WebUiEvent::PageClosed { page } => {
                remote.push_webui_page_closed(page.clone());
            }
            WebUiEvent::Notice { level, text } => {
                remote.push_webui_notice(level.clone(), text.clone());
            }
            WebUiEvent::Disconnected { .. } => {
                remote.push_webui_connected(false);
            }
            // Image results are a GUI-only concern (GUI image textures); the phone
            // fetches its own images over HTTP. Nothing to relay.
            WebUiEvent::ImageFetched { .. } => {}
        }
    }

    /// The registered WebUI pages (for a frontend's page picker).
    pub fn webui_pages(&self) -> &[crate::data::webui::WebUiPageDescriptor] {
        &self.webui_pages
    }

    /// Whether the bridge is live (a handshake started it this session).
    pub fn webui_is_active(&self) -> bool {
        self.webui_bridge.is_some()
    }

    /// Tear down the bridge (`.webui off`): drop the socket, clear state, and
    /// tell phone clients it disconnected. The handshake can be re-triggered.
    pub fn stop_webui(&mut self) {
        self.webui_bridge = None;
        self.webui_rx = None;
        self.webui_event_tx = None;
        self.webui_endpoint = None;
        self.webui_handshake_sent = false;
        self.webui_pages.clear();
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.push_webui_connected(false);
        }
    }
}
