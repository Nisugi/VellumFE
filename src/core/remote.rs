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
        command: String,
        color: Option<String>,
        confirm: bool,
        /// Set when editing: the button's previous (group, label).
        original: Option<(Option<String>, String)>,
    },
    /// Delete a phone-authored macro button by (group, label).
    MacroDelete {
        group: Option<String>,
        label: String,
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
}

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
}

/// Core-side producer for remote clients.
pub struct RemoteSink {
    buffer: Arc<Mutex<RemoteBuffer>>,
    delta_tx: broadcast::Sender<RemoteDelta>,
    state_tx: watch::Sender<RemoteStateSnapshot>,
    macros_tx: watch::Sender<Arc<RemoteMacros>>,
    /// State as of the previous flush, for change detection.
    last: RemoteStateSnapshot,
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
        let handles = RemoteServerHandles {
            buffer: buffer.clone(),
            delta_tx: delta_tx.clone(),
            state_rx,
            event_tx,
            macros_rx,
            session,
        };
        (
            Self {
                buffer,
                delta_tx,
                state_tx,
                macros_tx,
                last: RemoteStateSnapshot::default(),
            },
            handles,
            event_rx,
        )
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
    pub fn flush_state(&mut self, snap: RemoteStateSnapshot) {
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
        // server_time ticks on every prompt; only RT/CT end changes are
        // worth a delta (the client computes countdowns locally).
        if snap.roundtime_end != self.last.roundtime_end
            || snap.casttime_end != self.last.casttime_end
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
