//! WebSocket wire protocol for remote (phone browser) clients.
//!
//! Envelope: `{ "v": 1, "seq": n, "t": "...", "d": {...} }`. Every
//! server→client message carries a monotonically non-decreasing `seq`;
//! for `text` messages it is the line's own sequence number (the client's
//! reconnect-resume cursor), for state messages it is the newest line seq
//! known at send time. Colors inside `StyledLine` segments are already CSS
//! hex strings; see docs/mobile-web-frontend-plan.md for the full table.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::remote::{RemoteDelta, RemoteMacros, RemoteMenuItem, RemoteStateSnapshot};
use crate::core::state::{StatusInfo, Vitals};
use crate::data::remote_buffer::RemoteLine;
use crate::data::widget::StyledLine;

pub const PROTOCOL_VERSION: u8 = 1;

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    v: u8,
    seq: u64,
    t: &'static str,
    d: T,
}

fn encode<T: Serialize>(t: &'static str, seq: u64, d: T) -> String {
    serde_json::to_string(&Envelope {
        v: PROTOCOL_VERSION,
        seq,
        t,
        d,
    })
    .expect("protocol payloads always serialize")
}

#[derive(Serialize)]
struct HelloPayload {
    character: Option<String>,
    streams: Vec<String>,
    /// Process-instance id; seqs restart when it changes, so clients must
    /// drop their resume cursor on mismatch.
    session: String,
}

/// How the text in a snapshot relates to what the client already has.
#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotMode {
    /// Fresh view: client clears its pane and renders from scratch.
    Full,
    /// Successful resume: text contains only lines newer than the client's
    /// cursor; the client keeps its pane and appends.
    Resume,
    /// Resume failed (lines evicted): client keeps its pane, shows a
    /// "missed output" marker, then appends the snapshot tail.
    Gap,
}

#[derive(Serialize)]
struct TextPayload {
    stream: String,
    line: Arc<StyledLine>,
}

#[derive(Serialize)]
struct RoomPayload {
    name: Option<String>,
    exits: Vec<String>,
}

#[derive(Serialize)]
struct HandsPayload {
    left: Option<String>,
    right: Option<String>,
}

#[derive(Serialize)]
struct RtPayload {
    roundtime_end: Option<i64>,
    casttime_end: Option<i64>,
    server_time: i64,
}

#[derive(Serialize)]
struct MenuPayload<'a> {
    request_id: u64,
    noun: &'a str,
    items: &'a [RemoteMenuItem],
}

#[derive(Serialize)]
struct SnapshotLine {
    seq: u64,
    stream: String,
    line: Arc<StyledLine>,
}

#[derive(Serialize)]
struct SnapshotPayload {
    mode: SnapshotMode,
    character: Option<String>,
    vitals: Vitals,
    room: RoomPayload,
    hands: HandsPayload,
    indicators: StatusInfo,
    rt: RtPayload,
    text: Vec<SnapshotLine>,
}

/// First message on every connection.
pub fn hello(
    character: Option<String>,
    streams: Vec<String>,
    session: String,
    seq: u64,
) -> String {
    encode(
        "hello",
        seq,
        HelloPayload {
            character,
            streams,
            session,
        },
    )
}

/// Full state + scrollback (or resume replay, per `mode`); sent after the
/// client's `resume`, and when a client lags too far behind the broadcast.
pub fn snapshot(
    state: &RemoteStateSnapshot,
    lines: Vec<RemoteLine>,
    mode: SnapshotMode,
    seq: u64,
) -> String {
    let payload = SnapshotPayload {
        mode,
        character: state.character.clone(),
        vitals: state.vitals.clone(),
        room: RoomPayload {
            name: state.room_name.clone(),
            exits: state.exits.clone(),
        },
        hands: HandsPayload {
            left: state.left_hand.clone(),
            right: state.right_hand.clone(),
        },
        indicators: state.indicators.clone(),
        rt: RtPayload {
            roundtime_end: state.roundtime_end,
            casttime_end: state.casttime_end,
            server_time: state.server_time,
        },
        text: lines
            .into_iter()
            .map(|l| SnapshotLine {
                seq: l.seq,
                stream: l.stream,
                line: l.line,
            })
            .collect(),
    };
    encode("snapshot", seq, payload)
}

/// Encode a broadcast delta. `last_seq` is used as the envelope seq for
/// non-text deltas; text deltas carry their own line seq.
pub fn delta(delta: &RemoteDelta, last_seq: u64) -> String {
    match delta {
        RemoteDelta::Text(l) => encode(
            "text",
            l.seq,
            TextPayload {
                stream: l.stream.clone(),
                line: l.line.clone(),
            },
        ),
        RemoteDelta::Vitals(v) => encode("vitals", last_seq, v.clone()),
        RemoteDelta::Room { name, exits } => encode(
            "room",
            last_seq,
            RoomPayload {
                name: name.clone(),
                exits: exits.clone(),
            },
        ),
        RemoteDelta::Hands { left, right } => encode(
            "hands",
            last_seq,
            HandsPayload {
                left: left.clone(),
                right: right.clone(),
            },
        ),
        RemoteDelta::Indicators(status) => encode("indicators", last_seq, status.clone()),
        RemoteDelta::Rt {
            roundtime_end,
            casttime_end,
            server_time,
        } => encode(
            "rt",
            last_seq,
            RtPayload {
                roundtime_end: *roundtime_end,
                casttime_end: *casttime_end,
                server_time: *server_time,
            },
        ),
        // client_id stays server-side: the ws task already filtered on it.
        RemoteDelta::Menu {
            request_id,
            noun,
            items,
            ..
        } => encode(
            "menu",
            last_seq,
            MenuPayload {
                request_id: *request_id,
                noun,
                items,
            },
        ),
        RemoteDelta::Macros(m) => macros(m, last_seq),
    }
}

/// Macro definitions; sent on connect and after `.reloadmacros`.
pub fn macros(m: &RemoteMacros, seq: u64) -> String {
    encode("macros", seq, m)
}

/// Sent right before closing an unauthenticated connection, so the client
/// can show its pairing prompt instead of retry-looping.
pub fn denied() -> String {
    encode("denied", 0, serde_json::json!({}))
}

/// Messages a client may send. Unknown types are ignored (forward compat).
#[derive(Debug, PartialEq)]
pub enum ClientMessage {
    /// Pairing token; must be the first message on every connection.
    Auth { token: String },
    /// A typed command destined for the game (or a dot-command).
    Cmd { text: String },
    /// Resume request with the highest text seq the client has rendered
    /// (0 = fresh view).
    Resume { seq: u64 },
    /// A tapped link. Links with a coord (or `<d>` tags) resolve to their
    /// default command server-side; plain links issue `_menu` upstream and
    /// the response comes back as a `menu` message with this request_id.
    LinkTap {
        request_id: u64,
        exist_id: String,
        noun: String,
        text: String,
        coord: Option<String>,
    },
    /// A macro button/option tap; the id is resolved to its command
    /// server-side (the client never sends macro command text).
    Macro { id: String },
    /// Create/edit a phone-authored macro button (macros-local.toml).
    MacroSave {
        group: Option<String>,
        label: String,
        command: String,
        color: Option<String>,
        confirm: bool,
        options: Vec<crate::config::MacroOption>,
        original: Option<(Option<String>, String)>,
    },
    /// Delete a phone-authored macro button.
    MacroDelete {
        group: Option<String>,
        label: String,
    },
}

fn opt_str(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

#[derive(Deserialize)]
struct RawClientMessage {
    t: String,
    #[serde(default)]
    d: serde_json::Value,
}

/// Parse a client frame. Returns None for malformed or unknown messages.
pub fn parse_client_message(raw: &str) -> Option<ClientMessage> {
    let msg: RawClientMessage = serde_json::from_str(raw).ok()?;
    match msg.t.as_str() {
        "auth" => {
            let token = msg.d.get("token")?.as_str()?.to_string();
            Some(ClientMessage::Auth { token })
        }
        "cmd" => {
            let text = msg.d.get("text")?.as_str()?.to_string();
            Some(ClientMessage::Cmd { text })
        }
        "resume" => {
            let seq = msg.d.get("seq")?.as_u64()?;
            Some(ClientMessage::Resume { seq })
        }
        "link_tap" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let exist_id = msg.d.get("exist_id")?.as_str()?.to_string();
            let noun = msg.d.get("noun")?.as_str()?.to_string();
            let text = msg
                .d
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let coord = msg
                .d
                .get("coord")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            Some(ClientMessage::LinkTap {
                request_id,
                exist_id,
                noun,
                text,
                coord,
            })
        }
        "macro" => {
            let id = msg.d.get("id")?.as_str()?.to_string();
            Some(ClientMessage::Macro { id })
        }
        "macro_save" => {
            let label = msg.d.get("label")?.as_str()?.trim().to_string();
            let command = msg
                .d
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let options: Vec<crate::config::MacroOption> = msg
                .d
                .get("options")
                .and_then(|v| v.as_array())
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|o| {
                            let label = o.get("label")?.as_str()?.trim().to_string();
                            let command = o.get("command")?.as_str()?.trim().to_string();
                            if label.is_empty() || command.is_empty() {
                                return None;
                            }
                            Some(crate::config::MacroOption {
                                label,
                                command,
                                confirm: o.get("confirm").and_then(|v| v.as_bool()).unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            // A button needs a label and either a direct command or at
            // least one option (menu button).
            if label.is_empty() || (command.is_empty() && options.is_empty()) {
                return None;
            }
            let original = msg.d.get("original").filter(|v| !v.is_null()).and_then(|o| {
                Some((opt_str(o.get("group")), o.get("label")?.as_str()?.to_string()))
            });
            Some(ClientMessage::MacroSave {
                group: opt_str(msg.d.get("group")),
                label,
                command,
                color: opt_str(msg.d.get("color")),
                confirm: msg.d.get("confirm").and_then(|v| v.as_bool()).unwrap_or(false),
                options,
                original,
            })
        }
        "macro_delete" => {
            let label = msg.d.get("label")?.as_str()?.to_string();
            Some(ClientMessage::MacroDelete {
                group: opt_str(msg.d.get("group")),
                label,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::TextSegment;

    #[test]
    fn parse_client_cmd_and_resume() {
        assert_eq!(
            parse_client_message(r#"{"t":"cmd","d":{"text":"look"}}"#),
            Some(ClientMessage::Cmd {
                text: "look".to_string()
            })
        );
        assert_eq!(
            parse_client_message(r#"{"t":"resume","d":{"seq":41}}"#),
            Some(ClientMessage::Resume { seq: 41 })
        );
        assert_eq!(parse_client_message(r#"{"t":"unknown","d":{}}"#), None);
        assert_eq!(parse_client_message("not json"), None);
    }

    #[test]
    fn text_delta_uses_line_seq_and_expected_shape() {
        let line = Arc::new(StyledLine {
            segments: vec![TextSegment::plain("hi")],
            stream: "main".to_string(),
        });
        let d = RemoteDelta::Text(RemoteLine {
            seq: 42,
            stream: "main".to_string(),
            line,
        });
        let json: serde_json::Value = serde_json::from_str(&delta(&d, 99)).unwrap();
        assert_eq!(json["v"], 1);
        assert_eq!(json["seq"], 42);
        assert_eq!(json["t"], "text");
        assert_eq!(json["d"]["stream"], "main");
        assert_eq!(json["d"]["line"]["segments"][0]["text"], "hi");
    }

    #[test]
    fn snapshot_includes_state_and_lines() {
        let mut state = RemoteStateSnapshot::default();
        state.character = Some("Testy".to_string());
        state.vitals.health = 73;
        let lines = vec![RemoteLine {
            seq: 7,
            stream: "main".to_string(),
            line: Arc::new(StyledLine {
                segments: vec![TextSegment::plain("x")],
                stream: "main".to_string(),
            }),
        }];
        let json: serde_json::Value =
            serde_json::from_str(&snapshot(&state, lines, SnapshotMode::Full, 7)).unwrap();
        assert_eq!(json["t"], "snapshot");
        assert_eq!(json["d"]["mode"], "full");
        assert_eq!(json["d"]["character"], "Testy");
        assert_eq!(json["d"]["vitals"]["health"], 73);
        assert_eq!(json["d"]["text"][0]["seq"], 7);
    }
}
