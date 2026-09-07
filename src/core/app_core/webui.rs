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
    pub fn set_webui_gui_channel(&mut self, tx: tokio::sync::mpsc::UnboundedSender<WebUiEvent>) {
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

    /// Whether this session is proxied through Lich. Set false for direct
    /// eAccess. Frontends call this once the connection mode is known.
    ///
    /// Deliberately separate from `set_webui_available`: WebUI is an optional
    /// Lich feature that may be absent or torn down (`.webui off`), while a
    /// Lich *connection* is what makes `;` commands deliverable at all.
    /// Anything that sends a `;` command must gate on this, not on WebUI.
    pub fn set_lich_connected(&mut self, connected: bool) {
        self.lich_connected = connected;
    }

    /// Whether `;` commands can reach a Lich this session.
    pub fn lich_connected(&self) -> bool {
        self.lich_connected
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
    pub fn start_webui(&mut self, runtime: &tokio::runtime::Handle, handshake: &WebUiHandshake) {
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
        // Publish the upstream to the web server so its /webui/files/ proxy
        // can fetch file-backed images for phone clients. The cookie stays
        // server-side; phones authenticate with the pairing token only.
        if let Some(remote) = self.message_processor.remote.as_mut() {
            remote.set_webui_upstream(Some(crate::core::remote::WebUiUpstream::new(
                host.clone(),
                port,
                token.clone(),
            )));
        }
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
    pub fn webui_event_sender(&self) -> Option<tokio::sync::mpsc::UnboundedSender<WebUiEvent>> {
        self.webui_event_tx.clone()
    }

    /// Subscribe a consumer to a page on the bridge (a phone opening a
    /// panel, or the GUI's own panels as `WebUiConsumer::Local`). Consumers
    /// are a set, so duplicate subscribes from the same consumer are
    /// idempotent. The upstream subscribe is always forwarded — it is
    /// idempotent on Lich and triggers a fresh render for the joining
    /// consumer — and the page set replays when a fresh socket's Hello
    /// arrives (the bridge may not be up yet on the first subscribe).
    pub fn webui_subscribe(&mut self, consumer: crate::core::remote::WebUiConsumer, page: &str) {
        self.webui_subscribed
            .entry(page.to_string())
            .or_default()
            .insert(consumer);
        if let Some(bridge) = &self.webui_bridge {
            bridge.subscribe(page);
        }
    }

    /// Drop one consumer from a page. The upstream unsubscribe is sent only
    /// when the LAST consumer leaves — desktop and phone (or two phones)
    /// share pages, and closing one must not stop the other's updates.
    pub fn webui_unsubscribe(&mut self, consumer: crate::core::remote::WebUiConsumer, page: &str) {
        let last = match self.webui_subscribed.get_mut(page) {
            Some(consumers) => {
                consumers.remove(&consumer);
                consumers.is_empty()
            }
            // Unknown page: nothing tracked, nothing to release upstream.
            None => return,
        };
        if last {
            self.webui_subscribed.remove(page);
            if let Some(bridge) = &self.webui_bridge {
                bridge.send(WebUiClientMessage::Unsubscribe {
                    page: page.to_string(),
                });
            }
        }
    }

    /// A remote client's WebSocket ended (clean close or abrupt drop):
    /// remove every subscription it held and release upstream pages whose
    /// last consumer just left. Other consumers' state is untouched.
    pub fn webui_client_gone(&mut self, client_id: u64) {
        let consumer = crate::core::remote::WebUiConsumer::Remote(client_id);
        let mut released = Vec::new();
        self.webui_subscribed.retain(|page, consumers| {
            consumers.remove(&consumer);
            if consumers.is_empty() {
                released.push(page.clone());
                false
            } else {
                true
            }
        });
        if let Some(bridge) = &self.webui_bridge {
            for page in released {
                bridge.send(WebUiClientMessage::Unsubscribe { page });
            }
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
                    for page in self.webui_subscribed.keys() {
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
            // Withdraw the image-proxy upstream: after teardown the proxy
            // must answer "bridge unavailable", never dial the old endpoint.
            remote.set_webui_upstream(None);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Per-consumer WebUI subscription tracking (finding 10). These assert
    //! the UPSTREAM message transitions (what would be sent to Lich over the
    //! bridge socket) and delivery to remaining consumers, not just the
    //! internal collection.

    use super::*;
    use crate::core::remote::{RemoteDelta, RemoteSink, WebUiConsumer};
    use crate::data::webui::WebUiClientMessage;

    fn app_with_bridge() -> (
        AppCore,
        tokio::sync::mpsc::UnboundedReceiver<WebUiClientMessage>,
    ) {
        let mut core = AppCore::new_for_test();
        let (handle, upstream_rx) = crate::webui::WebUiHandle::test_pair();
        core.webui_bridge = Some(handle);
        (core, upstream_rx)
    }

    fn drain(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<WebUiClientMessage>,
    ) -> Vec<WebUiClientMessage> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            out.push(msg);
        }
        out
    }

    fn sub(page: &str) -> WebUiClientMessage {
        WebUiClientMessage::Subscribe {
            page: page.to_string(),
        }
    }

    fn unsub(page: &str) -> WebUiClientMessage {
        WebUiClientMessage::Unsubscribe {
            page: page.to_string(),
        }
    }

    /// Two remote clients share a page: the first leave sends NO upstream
    /// unsubscribe and renders keep flowing to the remaining client; the
    /// last leave releases the page upstream.
    #[test]
    fn two_remote_clients_last_consumer_releases_upstream() {
        let (mut core, mut up) = app_with_bridge();
        core.webui_subscribe(WebUiConsumer::Remote(1), "bigshot");
        core.webui_subscribe(WebUiConsumer::Remote(2), "bigshot");
        assert_eq!(drain(&mut up), vec![sub("bigshot"), sub("bigshot")]);

        // Client 1 closes its panel: no upstream unsubscribe.
        core.webui_unsubscribe(WebUiConsumer::Remote(1), "bigshot");
        assert_eq!(drain(&mut up), vec![]);

        // A render arriving now still reaches remote consumers.
        let (sink, handles, _events) = RemoteSink::new(16);
        core.message_processor.remote = Some(sink);
        let mut delta_rx = handles.delta_tx.subscribe();
        core.relay_webui_to_phone(&crate::webui::WebUiEvent::Render {
            page: "bigshot".to_string(),
            seq: 7,
            tree: Default::default(),
        });
        match delta_rx.try_recv() {
            Ok(RemoteDelta::WebUiRender { page, seq, .. }) => {
                assert_eq!(page, "bigshot");
                assert_eq!(seq, 7);
            }
            other => panic!("expected WebUiRender delta, got {other:?}"),
        }

        // Last consumer leaves: upstream unsubscribe fires exactly once.
        core.webui_unsubscribe(WebUiConsumer::Remote(2), "bigshot");
        assert_eq!(drain(&mut up), vec![unsub("bigshot")]);
    }

    /// Desktop panel + phone share a page: the phone disconnecting keeps the
    /// desktop's upstream subscription; closing the desktop panel afterwards
    /// releases it.
    #[test]
    fn local_plus_remote_share_page() {
        let (mut core, mut up) = app_with_bridge();
        core.webui_subscribe(WebUiConsumer::Local, "map/main");
        core.webui_subscribe(WebUiConsumer::Remote(9), "map/main");
        drain(&mut up);

        core.webui_client_gone(9);
        assert_eq!(
            drain(&mut up),
            vec![],
            "phone drop must not unsubscribe the desktop's page"
        );

        core.webui_unsubscribe(WebUiConsumer::Local, "map/main");
        assert_eq!(drain(&mut up), vec![unsub("map/main")]);
    }

    /// Repeated subscribes from one consumer are idempotent (a set, not a
    /// counter): one unsubscribe still releases the page.
    #[test]
    fn duplicate_subscribes_do_not_leak() {
        let (mut core, mut up) = app_with_bridge();
        for _ in 0..3 {
            core.webui_subscribe(WebUiConsumer::Remote(4), "bigshot");
        }
        drain(&mut up);
        core.webui_unsubscribe(WebUiConsumer::Remote(4), "bigshot");
        assert_eq!(drain(&mut up), vec![unsub("bigshot")]);
        assert!(core.webui_subscribed.is_empty());
    }

    /// Abrupt disconnect removes only that client's subscriptions, applying
    /// last-consumer transitions per page.
    #[test]
    fn abrupt_disconnect_releases_only_that_clients_pages() {
        let (mut core, mut up) = app_with_bridge();
        core.webui_subscribe(WebUiConsumer::Remote(1), "solo");
        core.webui_subscribe(WebUiConsumer::Remote(1), "shared");
        core.webui_subscribe(WebUiConsumer::Remote(2), "shared");
        core.webui_subscribe(WebUiConsumer::Remote(2), "other");
        drain(&mut up);

        core.webui_client_gone(1);
        let msgs = drain(&mut up);
        assert_eq!(
            msgs,
            vec![unsub("solo")],
            "only the sole-consumer page unsubscribes"
        );
        assert!(core.webui_subscribed.contains_key("shared"));
        assert!(core.webui_subscribed.contains_key("other"));

        // Gone again (duplicate disconnect event) is a no-op.
        core.webui_client_gone(1);
        assert_eq!(drain(&mut up), vec![]);
    }

    /// Unsubscribing a consumer that never subscribed, or an unknown page,
    /// sends nothing upstream.
    #[test]
    fn unsubscribe_without_subscription_is_silent() {
        let (mut core, mut up) = app_with_bridge();
        core.webui_unsubscribe(WebUiConsumer::Remote(1), "nope");
        core.webui_subscribe(WebUiConsumer::Remote(2), "page");
        drain(&mut up);
        // A different consumer unsubscribing does not release the page.
        core.webui_unsubscribe(WebUiConsumer::Remote(3), "page");
        assert_eq!(drain(&mut up), vec![]);
        assert!(core.webui_subscribed.contains_key("page"));
    }

    /// After a bridge reconnect (fresh socket Hello) the UNION of active
    /// subscriptions replays upstream, regardless of which consumer holds
    /// each page.
    #[test]
    fn bridge_reconnect_replays_union() {
        let (mut core, mut up) = app_with_bridge();
        core.webui_subscribe(WebUiConsumer::Local, "desktop-page");
        core.webui_subscribe(WebUiConsumer::Remote(1), "phone-page");
        core.webui_subscribe(WebUiConsumer::Remote(2), "phone-page");
        drain(&mut up);

        // Hello relaying requires the remote sink (phone fan-out) installed.
        let (sink, _handles, _events) = RemoteSink::new(16);
        core.message_processor.remote = Some(sink);
        core.relay_webui_to_phone(&crate::webui::WebUiEvent::Hello {
            schema_version: 1,
            session: Default::default(),
            pages: Vec::new(),
        });

        let mut pages: Vec<String> = drain(&mut up)
            .into_iter()
            .map(|m| match m {
                WebUiClientMessage::Subscribe { page } => page,
                other => panic!("expected only Subscribe replays, got {other:?}"),
            })
            .collect();
        pages.sort();
        assert_eq!(
            pages,
            vec!["desktop-page".to_string(), "phone-page".to_string()]
        );
    }
}
