//! Lich WebUI bridge: handshake, event pump, rendered-page application,
//! and opening/closing WebUI-backed windows.

use super::*;

impl VellumGuiApp {
    // ==================== Lich WebUI bridge ====================

    pub(super) fn has_webui_windows(&self) -> bool {
        self.app_core
            .ui_state
            .windows
            .values()
            .any(|w| matches!(w.content, WindowContent::WebUi(_)))
    }

    /// Asks Lich for the WebUI endpoint. The reply comes back on the game
    /// stream as one `<LichWebUI .../>` line (handled in pump_server_messages).
    pub(super) fn request_webui_handshake(&mut self) {
        if self.is_direct_connection {
            self.app_core.add_system_message(
                "The Lich WebUI needs a Lich proxy connection (direct connections bypass Lich).",
            );
            self.webui_pending.clear();
            return;
        }
        self.webui_handshake_sent = true;
        // Accounted raw send (byte counters, no dot-command re-interception),
        // matching every other outbound line.
        self.dispatch_raw_command(";ui handshake".to_string());
    }

    pub(super) fn handle_webui_handshake(&mut self, handshake: crate::data::webui::WebUiHandshake) {
        match handshake.status.as_str() {
            "ok" => {}
            "disabled" => {
                self.app_core.add_system_message(
                    "Lich WebUI is disabled. Run ;ui on (persists), then .webui again.",
                );
                self.webui_pending.clear();
                return;
            }
            other => {
                self.app_core.add_system_message(&format!(
                    "Lich WebUI is not running (status: {}). Check ;ui status in Lich.",
                    other
                ));
                self.webui_pending.clear();
                return;
            }
        }
        let Some(token) = handshake.token().map(String::from) else {
            self.app_core
                .add_system_message("Lich WebUI handshake had no auth token; cannot connect.");
            return;
        };

        // Replace any prior bridge (Lich restarts change port and token).
        // Core owns the socket now (so the phone renders the same trees). The
        // GUI receives bridge events on a re-emit channel core feeds from its
        // per-frame pump; a waking hop repaints egui so panel updates aren't
        // stuck waiting for an idle frame.
        self.webui_rx = None;
        self.webui_fetches_inflight.clear();
        self.app_core.start_webui(self._runtime.handle(), &handshake);

        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<crate::webui::WebUiEvent>();
        let (forward_tx, forward_rx) = mpsc::unbounded_channel::<crate::webui::WebUiEvent>();
        let waker_ctx = std::sync::Arc::clone(&self.repaint_ctx);
        self._runtime.spawn(async move {
            while let Some(event) = raw_rx.recv().await {
                if forward_tx.send(event).is_err() {
                    break;
                }
                if let Some(ctx) = waker_ctx.lock().ok().and_then(|slot| slot.clone()) {
                    ctx.request_repaint();
                }
            }
        });
        self.app_core.set_webui_gui_channel(raw_tx);
        self.webui_rx = Some(forward_rx);
        let (host, port) = handshake.endpoint();
        tracing::info!("WebUI bridge (core-owned) connecting to {}:{}", host, port);
    }

    /// Applies bridge events to panel windows. Called once per frame.
    pub(super) fn pump_webui_events(&mut self) {
        let Some(rx) = self.webui_rx.as_mut() else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        for event in events {
            match event {
                crate::webui::WebUiEvent::Hello { session, pages, .. } => {
                    self.webui_pages = pages;
                    self.refresh_webui_window_kinds();
                    self.set_webui_windows_connected(true);
                    // Fresh connection: failed images are worth retrying, and
                    // Loading entries are orphans of the previous socket.
                    if let Some(ctx) = self.repaint_ctx.lock().ok().and_then(|slot| slot.clone()) {
                        Self::clear_stale_webui_images(&ctx);
                    }
                    self.app_core.add_system_message(&format!(
                        "Lich WebUI connected ({} - {} page{}).",
                        session.name,
                        self.webui_pages.len(),
                        if self.webui_pages.len() == 1 { "" } else { "s" }
                    ));
                    // Re-subscribe every panel window (fresh socket has no
                    // subscriptions; renders re-arrive and clear stale trees).
                    let pages: Vec<String> = self
                        .app_core
                        .ui_state
                        .windows
                        .values()
                        .filter_map(|w| match &w.content {
                            WindowContent::WebUi(content) => Some(content.page_id.clone()),
                            _ => None,
                        })
                        .collect();
                    for page in pages {
                        self.app_core.webui_subscribe(&page);
                    }
                    let pending = std::mem::take(&mut self.webui_pending);
                    for action in pending {
                        match action {
                            WebUiPendingAction::Picker => self.open_webui_picker(),
                            WebUiPendingAction::Open(page) => self.open_webui_page(&page),
                        }
                    }
                }
                crate::webui::WebUiEvent::Pages(pages) => {
                    // A page we host may have just re-registered (script
                    // restart): subscribe again so it resumes.
                    let hosted_ended: Vec<String> = self
                        .app_core
                        .ui_state
                        .windows
                        .values()
                        .filter_map(|w| match &w.content {
                            WindowContent::WebUi(content) if content.ended.is_some() => {
                                Some(content.page_id.clone())
                            }
                            _ => None,
                        })
                        .collect();
                    for page in hosted_ended {
                        if pages.iter().any(|p| p.id == page) {
                            self.app_core.webui_subscribe(&page);
                        }
                    }
                    // Transient pages registered while we're connected open
                    // themselves: ";bigshot setup" pops its dialog without a
                    // .webui round-trip (and the window auto-closes on save,
                    // completing the dialog lifecycle). Declared panels stay
                    // opt-in - dock once via .webui, the layout brings them
                    // back - so a script restart never resurrects a panel
                    // window the user closed on purpose. Dialogs (a settings
                    // popup, e.g. map/settings) are on-demand supplemental
                    // pages: they open via the image_map right-click popup,
                    // never just because the script registered them. The
                    // hello baseline never auto-opens (connecting must not
                    // spawn windows).
                    let fresh: Vec<String> = pages
                        .iter()
                        .filter(|p| !matches!(p.kind.as_deref(), Some("panel") | Some("dialog")))
                        .filter(|p| !self.webui_pages.iter().any(|k| k.id == p.id))
                        .map(|p| p.id.clone())
                        .collect();
                    self.webui_pages = pages;
                    self.refresh_webui_window_kinds();
                    for page in fresh {
                        self.open_webui_page(&page);
                    }
                }
                crate::webui::WebUiEvent::Render { page, seq, tree } => {
                    self.apply_webui_render(&page, seq, tree);
                }
                crate::webui::WebUiEvent::PageClosed { page } => {
                    // Declared panels (kind: "panel") stay docked with a
                    // notice and resume when the script re-registers, so a
                    // crash or ;repository restart never silently removes a
                    // docked widget. Everything else (settings dialogs,
                    // pages with no kind hint) closes with its script.
                    let is_panel = self.app_core.ui_state.windows.values().any(|w| {
                        matches!(&w.content, WindowContent::WebUi(c)
                            if c.page_id == page && c.kind.as_deref() == Some("panel"))
                    });
                    if is_panel {
                        self.with_webui_window(&page, |content| {
                            content.ended = Some("The owning script exited.".to_string());
                        });
                    } else {
                        let names: Vec<String> = self
                            .app_core
                            .ui_state
                            .windows
                            .values()
                            .filter(|w| {
                                matches!(&w.content, WindowContent::WebUi(c)
                                    if c.page_id == page)
                            })
                            .map(|w| w.name.clone())
                            .collect();
                        if !names.is_empty() {
                            for name in &names {
                                self.app_core.remove_window(name);
                                self.app_core.layout.windows.retain(|w| w.name() != *name);
                            }
                            self.app_core.schedule_layout_autosave();
                            tracing::info!(
                                "WebUI page '{}' ended; closed transient panel",
                                page
                            );
                        }
                    }
                }
                crate::webui::WebUiEvent::Notice { level, text } => {
                    self.app_core
                        .add_system_message(&format!("[WebUI {}] {}", level, text));
                }
                crate::webui::WebUiEvent::Disconnected {
                    gave_up,
                    never_connected,
                } => {
                    self.set_webui_windows_connected(false);
                    if gave_up {
                        if never_connected {
                            // The endpoint refused every attempt: almost
                            // always the advertised host isn't reachable
                            // from this machine (WebUI bound to localhost
                            // on the Lich box, firewall, stale address).
                            let endpoint = self
                                .app_core
                                .webui_endpoint()
                                .map(|(host, port, _)| format!("{}:{}", host, port))
                                .unwrap_or_else(|| "the advertised address".to_string());
                            self.app_core.add_system_message(&format!(
                                "Lich WebUI unreachable at {} (every attempt refused). \
                                 If ';ui' says it is running, the WebUI is likely only \
                                 listening on the Lich machine's localhost or its \
                                 firewall blocks the port - check that http://{}/ opens \
                                 in a browser on THIS machine, then run .webui again.",
                                endpoint, endpoint
                            ));
                        } else {
                            self.app_core.add_system_message(
                                "Lich WebUI connection lost (Lich restarted?). Run .webui to reconnect.",
                            );
                        }
                        // Core owns the bridge; tear it down there. The GUI's
                        // own event channel is dropped so the pump goes quiet.
                        self.app_core.stop_webui();
                        self.webui_rx = None;
                        break;
                    }
                }
                crate::webui::WebUiEvent::ImageFetched { src, data } => {
                    self.webui_fetches_inflight.remove(&src);
                    let Some(ctx) = self.repaint_ctx.lock().ok().and_then(|slot| slot.clone())
                    else {
                        continue;
                    };
                    let state = match data {
                        Ok(bytes) => Self::decode_webui_image(&ctx, &src, &bytes),
                        Err(err) => {
                            tracing::warn!("WebUI image '{}' fetch failed: {}", src, err);
                            webui_panel::WebUiImageState::Failed(err)
                        }
                    };
                    Self::set_webui_image(&ctx, src, state);
                    self.app_core.needs_render = true;
                }
            }
        }
    }

    pub(super) fn apply_webui_render(&mut self, page: &str, seq: u64, tree: crate::data::webui::WebUiNode) {
        let mut applied = false;
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                if content.page_id == page {
                    // Drop stale out-of-order renders (reconnect replays can
                    // race a live push).
                    if seq != 0 && seq <= content.seq && content.tree.is_some() {
                        return;
                    }
                    content.tree = Some(tree);
                    content.seq = seq;
                    content.generation = content.generation.wrapping_add(1);
                    content.connected = true;
                    content.ended = None;
                    applied = true;
                    break;
                }
            }
        }
        if applied {
            self.app_core.needs_render = true;
        } else {
            tracing::debug!("WebUI render for unhosted page '{}' ignored", page);
        }
    }

    pub(super) fn with_webui_window(
        &mut self,
        page: &str,
        apply: impl FnOnce(&mut crate::data::webui::WebUiPanelContent),
    ) {
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                if content.page_id == page {
                    apply(content);
                    self.app_core.needs_render = true;
                    return;
                }
            }
        }
    }

    pub(super) fn set_webui_windows_connected(&mut self, connected: bool) {
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                content.connected = connected;
            }
        }
        self.app_core.needs_render = true;
    }

    /// Popup menu of the session's registered pages (like `.addwindow`).
    pub(super) fn open_webui_picker(&mut self) {
        if self.webui_pages.is_empty() {
            self.app_core.add_system_message(
                "No WebUI pages are registered. Start a script that opens one (e.g. ;webui-demo).",
            );
            return;
        }
        let items: Vec<PopupMenuItem> = self
            .webui_pages
            .iter()
            .map(|page| {
                let text = if page.title.is_empty() {
                    page.id.clone()
                } else {
                    format!("{} ({})", page.title, page.id)
                };
                PopupMenuItem {
                    text,
                    // Menu commands that miss every internal prefix are sent
                    // to the game raw (Wrayth menu semantics), so this must
                    // be the action string, not the ".webui" dot-command.
                    command: format!("action:webui:open:{}", page.id),
                    disabled: false,
                }
            })
            .collect();
        self.close_all_popup_menus();
        self.app_core.ui_state.popup_menu = Some(PopupMenu::new(items, (8, 4)));
        self.app_core.ui_state.input_mode = InputMode::Menu;
    }

    /// Refreshes each hosted window's remembered page kind from the current
    /// registry. Windows restored from a saved layout start with no kind;
    /// this is what re-teaches them before their page can end.
    pub(super) fn refresh_webui_window_kinds(&mut self) {
        for window in self.app_core.ui_state.windows.values_mut() {
            if let WindowContent::WebUi(content) = &mut window.content {
                if let Some(descriptor) =
                    self.webui_pages.iter().find(|p| p.id == content.page_id)
                {
                    if descriptor.kind.is_some() {
                        content.kind = descriptor.kind.clone();
                    }
                }
            }
        }
    }

    /// Creates (or focuses) the panel window for a page and subscribes.
    pub(super) fn open_webui_page(&mut self, page_id: &str) {
        let descriptor = self.webui_pages.iter().find(|p| p.id == page_id);
        let title = descriptor
            .map(|d| {
                if d.title.is_empty() {
                    d.id.clone()
                } else {
                    d.title.clone()
                }
            })
            .unwrap_or_else(|| page_id.to_string());
        let size = descriptor.and_then(|d| d.size);
        let kind = descriptor.and_then(|d| d.kind.clone());

        let name = self
            .app_core
            .add_webui_window(page_id, &title, size, kind.clone());
        self.with_webui_window(page_id, |content| {
            content.connected = true;
            if kind.is_some() {
                content.kind = kind.clone();
            }
        });
        self.app_core.webui_subscribe(page_id);
        self.layout_dirty = true;
        tracing::info!("WebUI panel '{}' opened for page '{}'", name, page_id);
    }

    /// Closes one WebUI window from its title-bar close button: removes the
    /// window and its layout entry, and unsubscribes the page so the server
    /// sees the viewer leave (scripts with a window watchdog - ;map - treat
    /// that as the window closing, matching the GTK/browser lifecycle).
    pub(super) fn close_webui_window(&mut self, name: &str) {
        let page_id = self.app_core.ui_state.windows.get(name).and_then(|w| {
            match &w.content {
                WindowContent::WebUi(content) => Some(content.page_id.clone()),
                _ => None,
            }
        });
        let Some(page_id) = page_id else { return };
        self.app_core.remove_window(name);
        self.app_core.layout.windows.retain(|w| w.name() != name);
        self.app_core.schedule_layout_autosave();
        self.layout_dirty = true;
        // Only unsubscribe when no other window still shows the page.
        let still_hosted = self.app_core.ui_state.windows.values().any(|w| {
            matches!(&w.content, WindowContent::WebUi(c) if c.page_id == page_id)
        });
        if !still_hosted {
            self.app_core.webui_unsubscribe(&page_id);
        }
        tracing::info!(
            "WebUI panel '{}' closed by user (page '{}', unsubscribed: {})",
            name,
            page_id,
            !still_hosted
        );
    }

    /// `.webui` action entry points (returns true when handled).
    pub(super) fn handle_webui_action(&mut self, action: &str) -> bool {
        if action == "action:webui" {
            if self.app_core.webui_is_active() && !self.app_core.webui_pages().is_empty() {
                self.open_webui_picker();
            } else {
                self.webui_pending.push(WebUiPendingAction::Picker);
                self.app_core
                    .add_system_message("Requesting WebUI handshake from Lich...");
                self.request_webui_handshake();
            }
            return true;
        }
        if action == "action:webui:off" {
            self.app_core.stop_webui();
            self.webui_rx = None;
            self.webui_pending.clear();
            self.webui_fetches_inflight.clear();
            self.set_webui_windows_connected(false);
            self.app_core
                .add_system_message("Lich WebUI bridge disconnected.");
            return true;
        }
        if let Some(page) = action.strip_prefix("action:webui:open:") {
            let page = page.to_string();
            if self.app_core.webui_is_active() {
                self.open_webui_page(&page);
            } else {
                self.webui_pending.push(WebUiPendingAction::Open(page));
                self.app_core
                    .add_system_message("Requesting WebUI handshake from Lich...");
                self.request_webui_handshake();
            }
            return true;
        }
        false
    }
}
