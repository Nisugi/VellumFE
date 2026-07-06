//! WebSocket wire protocol for remote (phone browser) clients.
//!
//! Envelope: `{ "v": 1, "seq": n, "t": "...", "d": {...} }`. Every
//! server→client message carries a monotonically non-decreasing `seq`;
//! for `text` messages it is the line's own sequence number (the client's
//! reconnect-resume cursor), for state messages it is the newest line seq
//! known at send time. Colors inside `StyledLine` segments are already CSS
//! hex strings; see docs/mobile-web-frontend-plan.md for the full table.

use std::sync::Arc;

use serde::Serialize;

use crate::core::remote::{RemoteDelta, RemoteStateSnapshot};
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
struct SnapshotLine {
    seq: u64,
    stream: String,
    line: Arc<StyledLine>,
}

#[derive(Serialize)]
struct SnapshotPayload {
    character: Option<String>,
    vitals: Vitals,
    room: RoomPayload,
    hands: HandsPayload,
    indicators: StatusInfo,
    rt: RtPayload,
    text: Vec<SnapshotLine>,
}

/// First message on every connection.
pub fn hello(character: Option<String>, streams: Vec<String>, seq: u64) -> String {
    encode("hello", seq, HelloPayload { character, streams })
}

/// Full state + recent scrollback; sent on connect and when a client lags
/// too far behind the broadcast channel.
pub fn snapshot(state: &RemoteStateSnapshot, lines: Vec<RemoteLine>, seq: u64) -> String {
    let payload = SnapshotPayload {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::TextSegment;

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
        let json: serde_json::Value = serde_json::from_str(&snapshot(&state, lines, 7)).unwrap();
        assert_eq!(json["t"], "snapshot");
        assert_eq!(json["d"]["character"], "Testy");
        assert_eq!(json["d"]["vitals"]["health"], 73);
        assert_eq!(json["d"]["text"][0]["seq"], 7);
    }
}
