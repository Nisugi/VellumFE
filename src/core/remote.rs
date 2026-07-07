//! Remote client plumbing: the core-owned end of the web frontend sidecar.
//!
//! `RemoteSink` lives inside `MessageProcessor` as an `Option` (None when
//! `[web]` is disabled — the cost is one branch per finalized line). It:
//!
//! - pushes finalized, styled-but-unwrapped lines into the shared ring
//!   buffer (`data/remote_buffer.rs`) and broadcasts each as a
//!   `RemoteDelta::Text`, sharing one `Arc<StyledLine>` between both
//! - flushes coalesced state deltas (vitals, room, hands, indicators,
//!   roundtime) once per message batch by diffing against the last flush
//!
//! The web server task holds the other ends (`RemoteServerHandles`): a
//! `broadcast::Receiver` per client, the shared buffer and a `watch` of the
//! latest state for connect-time snapshots. Channels and this small shared
//! ring are the only coupling — the server never touches `AppCore`.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::{broadcast, mpsc, watch};

use crate::config::MacrosConfig;
use crate::data::remote_buffer::{RemoteBuffer, RemoteLine};
use crate::data::widget::StyledLine;

use super::state::{GameState, StatusInfo, Vitals};

/// Broadcast channel capacity. Slow/disconnected clients that fall more
/// than this many deltas behind get `Lagged` and re-snapshot.
pub const DELTA_CHANNEL_CAPACITY: usize = 1024;

/// Where a `_menu` request originated. The game's `<menu>` response is
/// routed back to its origin: the local popup, or one remote client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuOrigin {
    Local,
    Remote { client_id: u64, request_id: u64 },
}

/// One entry of a game menu serialized for a remote client. `command` is
/// the cmdlist-substituted game command; the client executes a pick by
/// sending it back over the ordinary `cmd` path (no server-side menu
/// state). Disabled items are section headers from flattened submenus.
#[derive(Clone, Debug, Serialize)]
pub struct RemoteMenuItem {
    pub text: String,
    pub command: String,
    pub disabled: bool,
}

/// Macro buttons serialized for remote clients: ids and labels only —
/// commands stay server-side and are resolved by id on activation
/// (`MacrosConfig::resolve`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteMacros {
    pub groups: Vec<RemoteMacroGroup>,
    pub floating: Vec<RemoteMacroButton>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMacroGroup {
    pub name: String,
    pub buttons: Vec<RemoteMacroButton>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMacroButton {
    /// Index path into the current config (e.g. "g:0:b:2", "f:1").
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub confirm: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<RemoteMacroOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// Phone-authored (macros-local.toml): may be edited/deleted remotely.
    pub editable: bool,
    /// The command behind an editable action button, echoed back so the
    /// phone editor can prefill its form. Hand-file commands stay private.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMacroOption {
    pub id: String,
    pub label: String,
    pub confirm: bool,
    /// Echoed for phone-authored buttons only, so the editor can prefill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl RemoteMacros {
    pub fn from_config(config: &MacrosConfig) -> Self {
        fn wire_button(button: &crate::config::MacroButton, id: String) -> RemoteMacroButton {
            RemoteMacroButton {
                options: button
                    .options
                    .iter()
                    .enumerate()
                    .map(|(oi, option)| RemoteMacroOption {
                        id: format!("{id}:o:{oi}"),
                        label: option.label.clone(),
                        confirm: option.confirm,
                        command: if button.editable {
                            Some(option.command.clone())
                        } else {
                            None
                        },
                    })
                    .collect(),
                id,
                label: button.label.clone(),
                color: button.color.clone(),
                confirm: button.confirm,
                x: button.x,
                y: button.y,
                editable: button.editable,
                command: if button.editable {
                    button.command.clone()
                } else {
                    None
                },
            }
        }
        Self {
            groups: config
                .groups
                .iter()
                .enumerate()
                .map(|(gi, group)| RemoteMacroGroup {
                    name: group.name.clone(),
                    buttons: group
                        .buttons
                        .iter()
                        .enumerate()
                        .map(|(bi, b)| wire_button(b, format!("g:{gi}:b:{bi}")))
                        .collect(),
                })
                .collect(),
            floating: config
                .floating
                .iter()
                .enumerate()
                .map(|(fi, b)| wire_button(b, format!("f:{fi}")))
                .collect(),
        }
    }
}

/// Where the game session itself stands. Owned by the runtime that manages
/// the connection (the headless supervisor); TUI/GUI sidecar sessions stay
/// `Connected`-shaped implicitly and never send these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// No connection and none in progress; waiting for a login.
    #[default]
    Idle,
    /// eAccess authentication in flight.
    Authenticating,
    /// Authenticated; connecting to the game server.
    Connecting,
    Connected,
    /// Lost the connection; the supervisor is retrying.
    Reconnecting,
    /// Ended (auth failure or unrecoverable); shows `error`.
    Disconnected,
}

/// Session status mirrored to web clients (snapshot field + `session`
/// delta). `session_control` is the capability flag: true only when the
/// serving runtime accepts Connect/Disconnect (headless), so sidecar
/// sessions never render a login screen.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteSessionInfo {
    pub state: SessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub session_control: bool,
}

/// A state change broadcast to all connected remote clients.
#[derive(Clone, Debug)]
pub enum RemoteDelta {
    Text(RemoteLine),
    Vitals(Vitals),
    Room {
        name: Option<String>,
        exits: Vec<String>,
    },
    Hands {
        left: Option<String>,
        right: Option<String>,
    },
    Indicators(StatusInfo),
    Rt {
        roundtime_end: Option<i64>,
        casttime_end: Option<i64>,
        server_time: i64,
    },
    /// A `<menu>` response for one remote client's link tap. Broadcast to
    /// all server tasks; each forwards it only to its own client.
    Menu {
        client_id: u64,
        request_id: u64,
        noun: String,
        items: Vec<RemoteMenuItem>,
    },
    /// Macro definitions changed (`.reloadmacros`); sent to every client.
    Macros(Arc<RemoteMacros>),
    /// Active effects changed (spells/buffs/debuffs/cooldowns), in fixed
    /// category order.
    Effects(Vec<crate::data::ActiveEffectsContent>),
    /// Game-session status changed (headless runtime only).
    Session(RemoteSessionInfo),
    /// A highlight-triggered sound. Clients fetch the file from /sounds/
    /// and play it locally (the Android build has no native audio; the
    /// phone's browser engine is the sound device).
    Sound { file: String, volume: Option<f32> },
    /// Reply to one client's config get/put (addressed like `Menu`).
    /// `content` is set for reads; `error` for validation/IO failures;
    /// `saved` for successful writes.
    ConfigFile {
        client_id: u64,
        request_id: u64,
        file: String,
        content: Option<String>,
        error: Option<String>,
        saved: bool,
    },
    /// Reply to one client's structured highlight get/put/delete: the full
    /// rule map for the scope (or an error), plus the available sound
    /// files for the editor's dropdown.
    Highlights {
        client_id: u64,
        request_id: u64,
        scope: String,
        rules: serde_json::Value,
        sounds: Vec<String>,
        error: Option<String>,
    },
}

/// Input from a remote client, drained by the active frontend's main loop
/// (TUI runtime loop / GUI pump) and fed through the same command path as
/// locally typed input.
#[derive(Clone, Debug)]
pub enum RemoteEvent {
    /// A command typed on a remote client.
    Command(String),
    /// A link tapped on a remote client. The main loop resolves it exactly
    /// like a local click (AppCore::resolve_link_activation): `<d>` tags
    /// and coord links become direct commands; plain links become a
    /// `_menu` request tagged with the origin.
    LinkTap {
        client_id: u64,
        request_id: u64,
        exist_id: String,
        noun: String,
        text: String,
        coord: Option<String>,
    },
    /// A macro button/option tapped on a remote client. The main loop
    /// resolves the id against config (MacrosConfig::resolve) and runs
    /// the command through the same dispatch as typed input.
    Macro { id: String },
    /// Create or edit a phone-authored macro button (lands in the
    /// macros-local.toml overlay; AppCore::apply_macro_save).
    MacroSave {
        /// Target rail group by name; None = floating.
        group: Option<String>,
        label: String,
        /// Empty when the button is a menu (options-only) button.
        command: String,
        color: Option<String>,
        confirm: bool,
        /// Non-empty makes this a menu button (tap opens the sheet).
        options: Vec<crate::config::MacroOption>,
        /// Set when editing: the button's previous (group, label).
        original: Option<(Option<String>, String)>,
    },
    /// Delete a phone-authored macro button by (group, label).
    MacroDelete {
        group: Option<String>,
        label: String,
    },
    /// A status notice from the web server task for the local UI (e.g.
    /// "bound port 8041" or "pinned port taken, web disabled"). The main
    /// loop surfaces it as a system message.
    Notice(String),
    /// A login request from a web client (headless runtime only; TUI/GUI
    /// reply with a notice). Either a saved profile name, or inline
    /// credentials that optionally get saved as a profile.
    SessionConnect {
        profile: Option<String>,
        account: Option<String>,
        password: Option<String>,
        character: Option<String>,
        game: Option<String>,
        save_password: bool,
        profile_name: Option<String>,
    },
    /// User-initiated disconnect: end the session, suppress reconnection.
    SessionDisconnect,
    /// Read a whitelisted config file (settings sheet editor). The reply
    /// routes back to the requesting client as `RemoteDelta::ConfigFile`.
    ConfigGet {
        client_id: u64,
        request_id: u64,
        file: String,
    },
    /// Validate and write a whitelisted config file, then hot-reload it.
    ConfigPut {
        client_id: u64,
        request_id: u64,
        file: String,
        content: String,
    },
    /// Structured highlight-rule listing for the phone editor. `scope` is
    /// "profile" or "global".
    HighlightsGet {
        client_id: u64,
        request_id: u64,
        scope: String,
    },
    /// Create/update one highlight rule by name (JSON matching
    /// HighlightPattern); replies with the full updated rule map.
    HighlightPut {
        client_id: u64,
        request_id: u64,
        scope: String,
        name: String,
        rule: serde_json::Value,
    },
    /// Delete one highlight rule by name; replies with the updated map.
    HighlightDelete {
        client_id: u64,
        request_id: u64,
        scope: String,
        name: String,
    },
}

/// Latest coalesced game state, published via `watch` so the server can
/// build a connect-time snapshot without asking the main loop.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteStateSnapshot {
    pub character: Option<String>,
    pub vitals: Vitals,
    pub room_name: Option<String>,
    pub exits: Vec<String>,
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
    pub indicators: StatusInfo,
    pub roundtime_end: Option<i64>,
    pub casttime_end: Option<i64>,
    pub server_time: i64,
    /// Active effects in fixed category order (empty categories omitted).
    pub effects: Vec<crate::data::ActiveEffectsContent>,
    /// Session status + session-control capability. Overlaid by the sink in
    /// `flush_state` (the sink owns it, not GameState).
    pub session: RemoteSessionInfo,
}

/// Category display order for effects sent to clients.
pub const EFFECT_CATEGORIES: [&str; 4] = ["ActiveSpells", "Buffs", "Debuffs", "Cooldowns"];

impl RemoteStateSnapshot {
    /// The parts sourced directly from GameState. Callers layer on the
    /// fields that need context GameState doesn't have (room name from
    /// the streamWindow subtitle, exits from the compass, character from
    /// config).
    pub fn from_game_state(game_state: &GameState) -> Self {
        Self {
            character: game_state.character_name.clone(),
            vitals: game_state.vitals.clone(),
            room_name: game_state.room_name.clone(),
            exits: game_state.exits.clone(),
            left_hand: game_state.left_hand.clone(),
            right_hand: game_state.right_hand.clone(),
            indicators: game_state.status.clone(),
            roundtime_end: game_state.roundtime_end,
            casttime_end: game_state.casttime_end,
            server_time: game_state.game_time,
            effects: EFFECT_CATEGORIES
                .iter()
                .filter_map(|category| game_state.effects.get(*category))
                .cloned()
                .collect(),
            session: RemoteSessionInfo::default(),
        }
    }
}

/// Everything the web server task needs; returned by [`RemoteSink::new`].
#[derive(Clone)]
pub struct RemoteServerHandles {
    pub buffer: Arc<Mutex<RemoteBuffer>>,
    pub delta_tx: broadcast::Sender<RemoteDelta>,
    pub state_rx: watch::Receiver<RemoteStateSnapshot>,
    /// Client input flowing toward the main loop.
    pub event_tx: mpsc::UnboundedSender<RemoteEvent>,
    /// Latest macro definitions, for connect-time delivery.
    pub macros_rx: watch::Receiver<Arc<RemoteMacros>>,
    /// Identifies this process instance. Sent in `hello`; clients discard
    /// their resume cursor when it changes (seqs restart with the process).
    pub session: String,
    /// Set by the server task once it binds (unpinned instances may walk
    /// past the configured port). Read by `.webinfo`.
    pub bound_port: Arc<std::sync::OnceLock<u16>>,
}

/// Core-side producer for remote clients.
pub struct RemoteSink {
    buffer: Arc<Mutex<RemoteBuffer>>,
    delta_tx: broadcast::Sender<RemoteDelta>,
    state_tx: watch::Sender<RemoteStateSnapshot>,
    macros_tx: watch::Sender<Arc<RemoteMacros>>,
    bound_port: Arc<std::sync::OnceLock<u16>>,
    /// State as of the previous flush, for change detection.
    last: RemoteStateSnapshot,
    /// Session status owned by the serving runtime (headless supervisor);
    /// overlaid onto every snapshot/flush.
    session: RemoteSessionInfo,
}

impl RemoteSink {
    pub fn new(
        max_lines_per_stream: usize,
    ) -> (
        Self,
        RemoteServerHandles,
        mpsc::UnboundedReceiver<RemoteEvent>,
    ) {
        let buffer = Arc::new(Mutex::new(RemoteBuffer::new(max_lines_per_stream)));
        let (delta_tx, _) = broadcast::channel(DELTA_CHANNEL_CAPACITY);
        let (state_tx, state_rx) = watch::channel(RemoteStateSnapshot::default());
        let (macros_tx, macros_rx) = watch::channel(Arc::new(RemoteMacros::default()));
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let session = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let bound_port = Arc::new(std::sync::OnceLock::new());
        let handles = RemoteServerHandles {
            buffer: buffer.clone(),
            delta_tx: delta_tx.clone(),
            state_rx,
            event_tx,
            macros_rx,
            session,
            bound_port: bound_port.clone(),
        };
        (
            Self {
                buffer,
                delta_tx,
                state_tx,
                macros_tx,
                bound_port,
                last: RemoteStateSnapshot::default(),
                session: RemoteSessionInfo::default(),
            },
            handles,
            event_rx,
        )
    }

    /// Declare that this runtime accepts Connect/Disconnect from clients
    /// (headless only). Broadcast so already-connected clients learn the
    /// capability; also carried by every snapshot.
    pub fn set_session_control(&mut self, enabled: bool) {
        if self.session.session_control != enabled {
            self.session.session_control = enabled;
            self.publish_session();
        }
    }

    /// Publish a session status change (state machine transitions in the
    /// headless supervisor). Broadcast immediately — session changes must
    /// not wait for the next game-text batch — and folded into the watch
    /// so connect-time snapshots agree.
    pub fn set_session_state(&mut self, mut info: RemoteSessionInfo) {
        info.session_control = self.session.session_control;
        if self.session == info {
            return;
        }
        self.session = info;
        self.publish_session();
    }

    fn publish_session(&mut self) {
        let _ = self
            .delta_tx
            .send(RemoteDelta::Session(self.session.clone()));
        self.state_tx.send_modify(|snap| {
            snap.session = self.session.clone();
        });
        self.last.session = self.session.clone();
    }

    /// The port the server actually bound (may differ from config when an
    /// unpinned instance walked past a taken port). None until bound.
    pub fn bound_port(&self) -> Option<u16> {
        self.bound_port.get().copied()
    }

    /// Publish macro definitions: stored for connect-time delivery and
    /// broadcast to already-connected clients. Called on enable and by
    /// `.reloadmacros`.
    pub fn set_macros(&mut self, config: &MacrosConfig) {
        let macros = Arc::new(RemoteMacros::from_config(config));
        self.macros_tx.send_replace(macros.clone());
        let _ = self.delta_tx.send(RemoteDelta::Macros(macros));
    }

    /// Record a finalized (highlighted, unwrapped) line and broadcast it.
    /// The ring and the broadcast share the same `Arc<StyledLine>`.
    pub fn push_text(&mut self, stream: &str, line: Arc<StyledLine>) {
        let seq = self
            .buffer
            .lock()
            .expect("remote buffer lock poisoned")
            .push(stream, line.clone());
        // send() only fails when no client is subscribed; that's fine —
        // the ring still recorded the line for future snapshots.
        let _ = self.delta_tx.send(RemoteDelta::Text(RemoteLine {
            seq,
            stream: stream.to_string(),
            line,
        }));
    }

    /// Broadcast a highlight-triggered sound for clients to play.
    pub fn push_sound(&mut self, file: &str, volume: Option<f32>) {
        let _ = self.delta_tx.send(RemoteDelta::Sound {
            file: file.to_string(),
            volume,
        });
    }

    /// Route a config get/put reply to the remote client that requested it.
    #[allow(clippy::too_many_arguments)]
    pub fn push_config_file(
        &mut self,
        client_id: u64,
        request_id: u64,
        file: String,
        content: Option<String>,
        error: Option<String>,
        saved: bool,
    ) {
        let _ = self.delta_tx.send(RemoteDelta::ConfigFile {
            client_id,
            request_id,
            file,
            content,
            error,
            saved,
        });
    }

    /// Route a structured highlights reply to the requesting client.
    #[allow(clippy::too_many_arguments)]
    pub fn push_highlights(
        &mut self,
        client_id: u64,
        request_id: u64,
        scope: String,
        rules: serde_json::Value,
        sounds: Vec<String>,
        error: Option<String>,
    ) {
        let _ = self.delta_tx.send(RemoteDelta::Highlights {
            client_id,
            request_id,
            scope,
            rules,
            sounds,
            error,
        });
    }

    /// Route a game menu response to the remote client that requested it.
    pub fn push_menu(
        &mut self,
        client_id: u64,
        request_id: u64,
        noun: String,
        items: Vec<RemoteMenuItem>,
    ) {
        let _ = self.delta_tx.send(RemoteDelta::Menu {
            client_id,
            request_id,
            noun,
            items,
        });
    }

    /// Diff a freshly built state snapshot against the last flush and
    /// broadcast one coalesced delta per changed group. Called once per
    /// message batch (AppCore::flush_remote_state builds the snapshot —
    /// room name and exits need fallbacks only AppCore can see).
    pub fn flush_state(&mut self, mut snap: RemoteStateSnapshot) {
        // The sink owns session status; AppCore builds snapshots from
        // GameState which knows nothing about it.
        snap.session = self.session.clone();
        if snap == self.last {
            return;
        }

        if snap.vitals != self.last.vitals {
            let _ = self.delta_tx.send(RemoteDelta::Vitals(snap.vitals.clone()));
        }
        if snap.room_name != self.last.room_name || snap.exits != self.last.exits {
            let _ = self.delta_tx.send(RemoteDelta::Room {
                name: snap.room_name.clone(),
                exits: snap.exits.clone(),
            });
        }
        if snap.left_hand != self.last.left_hand || snap.right_hand != self.last.right_hand {
            let _ = self.delta_tx.send(RemoteDelta::Hands {
                left: snap.left_hand.clone(),
                right: snap.right_hand.clone(),
            });
        }
        if snap.indicators != self.last.indicators {
            let _ = self
                .delta_tx
                .send(RemoteDelta::Indicators(snap.indicators.clone()));
        }
        if snap.effects != self.last.effects {
            let _ = self
                .delta_tx
                .send(RemoteDelta::Effects(snap.effects.clone()));
        }
        // Send on RT/CT end changes AND on every prompt (server_time
        // tick). The per-prompt resend matters: a <roundTime> can be
        // flushed before its paired prompt is parsed, so the first delta
        // may carry a stale server_time and overstate the countdown by
        // seconds; the next prompt's delta corrects the client's clock
        // offset immediately - exactly how the TUI recalibrates
        // server_time_offset on every prompt.
        if snap.roundtime_end != self.last.roundtime_end
            || snap.casttime_end != self.last.casttime_end
            || snap.server_time != self.last.server_time
        {
            let _ = self.delta_tx.send(RemoteDelta::Rt {
                roundtime_end: snap.roundtime_end,
                casttime_end: snap.casttime_end,
                server_time: snap.server_time,
            });
        }

        self.state_tx.send_replace(snap.clone());
        self.last = snap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::TextSegment;

    fn styled(text: &str) -> Arc<StyledLine> {
        Arc::new(StyledLine {
            segments: vec![TextSegment::plain(text)],
            stream: "main".to_string(),
        })
    }

    #[test]
    fn push_text_buffers_and_broadcasts_shared_line() {
        let (mut sink, handles, _event_rx) = RemoteSink::new(100);
        let mut rx = handles.delta_tx.subscribe();

        sink.push_text("main", styled("hello"));

        let delta = rx.try_recv().expect("text delta should be broadcast");
        let RemoteDelta::Text(remote_line) = delta else {
            panic!("expected text delta");
        };
        assert_eq!(remote_line.seq, 1);
        assert_eq!(remote_line.stream, "main");

        let buf = handles.buffer.lock().unwrap();
        let tail = buf.tail("main", 10);
        assert_eq!(tail.len(), 1);
        // Ring and broadcast share the same allocation.
        assert!(Arc::ptr_eq(&tail[0].line, &remote_line.line));
    }

    #[test]
    fn flush_state_sends_only_changed_groups() {
        let (mut sink, handles, _event_rx) = RemoteSink::new(100);
        let mut rx = handles.delta_tx.subscribe();

        let mut gs = GameState::new();
        gs.vitals.health = 50;
        sink.flush_state(RemoteStateSnapshot::from_game_state(&gs));

        // Vitals changed relative to the default snapshot; room/hands/rt
        // did not (all None/empty in both).
        let delta = rx.try_recv().expect("vitals delta");
        assert!(matches!(delta, RemoteDelta::Vitals(v) if v.health == 50));
        assert!(rx.try_recv().is_err(), "no further deltas expected");

        // No change => no deltas at all.
        sink.flush_state(RemoteStateSnapshot::from_game_state(&gs));
        assert!(rx.try_recv().is_err());

        // Watch holds the latest state for snapshots.
        assert_eq!(handles.state_rx.borrow().vitals.health, 50);
    }

    #[test]
    fn flush_state_resyncs_clock_on_prompt_tick() {
        let (mut sink, handles, _event_rx) = RemoteSink::new(100);
        let mut rx = handles.delta_tx.subscribe();

        let mut gs = GameState::new();
        gs.game_time = 1000;
        sink.flush_state(RemoteStateSnapshot::from_game_state(&gs));
        while rx.try_recv().is_ok() {}

        // A prompt tick alone (no RT/CT change) must still emit an Rt
        // delta: clients recalibrate their clock offset from it, which is
        // what corrects a roundtime that was flushed before its paired
        // prompt was parsed.
        gs.game_time = 1002;
        sink.flush_state(RemoteStateSnapshot::from_game_state(&gs));
        let mut saw_resync = false;
        while let Ok(delta) = rx.try_recv() {
            if matches!(
                delta,
                RemoteDelta::Rt {
                    server_time: 1002,
                    ..
                }
            ) {
                saw_resync = true;
            }
        }
        assert!(saw_resync, "prompt tick should emit an Rt clock resync");
    }

    #[test]
    fn flush_state_rt_delta_on_roundtime_change() {
        let (mut sink, handles, _event_rx) = RemoteSink::new(100);
        let mut rx = handles.delta_tx.subscribe();

        let mut gs = GameState::new();
        gs.vitals = Vitals::default();
        sink.flush_state(RemoteStateSnapshot::from_game_state(&gs));
        while rx.try_recv().is_ok() {}

        gs.roundtime_end = Some(1_700_000_010);
        gs.game_time = 1_700_000_000;
        sink.flush_state(RemoteStateSnapshot::from_game_state(&gs));

        let mut saw_rt = false;
        while let Ok(delta) = rx.try_recv() {
            if let RemoteDelta::Rt {
                roundtime_end,
                server_time,
                ..
            } = delta
            {
                assert_eq!(roundtime_end, Some(1_700_000_010));
                assert_eq!(server_time, 1_700_000_000);
                saw_rt = true;
            }
        }
        assert!(saw_rt, "expected an Rt delta");
        drop(handles);
    }
}
