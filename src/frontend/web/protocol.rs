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

use crate::core::remote::{
    RemoteCharInfo, RemoteDelta, RemoteMacros, RemoteMenuItem, RemoteSessionInfo,
    RemoteStateSnapshot, RemoteTarget,
};
use crate::core::state::{StatusInfo, Vitals};
use crate::data::remote_buffer::RemoteLine;
use crate::data::widget::{ActiveEffectsContent, StyledLine};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
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
    effects: Vec<ActiveEffectsContent>,
    injuries: std::collections::HashMap<String, u8>,
    targets: Vec<RemoteTarget>,
    char_info: RemoteCharInfo,
    session: RemoteSessionInfo,
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
            id: state.room_id.clone(),
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
        effects: state.effects.clone(),
        injuries: state.injuries.clone(),
        targets: state.targets.clone(),
        char_info: state.char_info.clone(),
        session: state.session.clone(),
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
        RemoteDelta::Room { name, exits, id } => encode(
            "room",
            last_seq,
            RoomPayload {
                name: name.clone(),
                exits: exits.clone(),
                id: id.clone(),
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
        RemoteDelta::Effects(effects) => encode("effects", last_seq, effects),
        RemoteDelta::Session(info) => encode("session", last_seq, info),
        RemoteDelta::Injuries(injuries) => encode("injuries", last_seq, injuries),
        RemoteDelta::Targets(targets) => encode("targets", last_seq, targets),
        RemoteDelta::CharInfo(info) => encode("charinfo", last_seq, info),
        RemoteDelta::Sound { file, volume } => encode(
            "sound",
            last_seq,
            serde_json::json!({ "file": file, "volume": volume }),
        ),
        RemoteDelta::Colors {
            request_id,
            scope,
            colors,
            error,
            saved,
            ..
        } => encode(
            "colors",
            last_seq,
            serde_json::json!({
                "request_id": request_id,
                "scope": scope,
                "colors": colors,
                "error": error,
                "saved": saved,
            }),
        ),
        RemoteDelta::Highlights {
            request_id,
            scope,
            rules,
            sounds,
            error,
            ..
        } => encode(
            "highlights",
            last_seq,
            serde_json::json!({
                "request_id": request_id,
                "scope": scope,
                "rules": rules,
                "sounds": sounds,
                "error": error,
            }),
        ),
        // client_id stays server-side: the ws task already filtered on it.
        RemoteDelta::ConfigFile {
            request_id,
            file,
            content,
            error,
            saved,
            ..
        } => encode(
            "config_file",
            last_seq,
            serde_json::json!({
                "request_id": request_id,
                "file": file,
                "content": content,
                "error": error,
                "saved": saved,
            }),
        ),
    }
}

/// One saved login shown on the session screen. Never carries the password
/// or the full account name — only whether a password is stored.
#[derive(Serialize)]
pub struct ProfileEntry {
    pub name: String,
    /// "direct" or "lich".
    pub mode: String,
    pub account_masked: String,
    pub character: String,
    pub game: String,
    pub has_password: bool,
    /// Lich target; absent on direct profiles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// Saved-profile list; direct reply to a `get_profiles` request.
pub fn profiles(list: &[ProfileEntry], seq: u64) -> String {
    encode("profiles", seq, serde_json::json!({ "list": list }))
}

/// Mask an account name for display: first two characters + asterisks.
pub fn mask_account(account: &str) -> String {
    let visible: String = account.chars().take(2).collect();
    format!("{visible}{}", "*".repeat(account.chars().count().saturating_sub(2)))
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
    /// Start a game session (headless runtime only). Either a saved profile
    /// name, or inline credentials optionally saved as a new profile.
    Connect {
        profile: Option<String>,
        account: Option<String>,
        password: Option<String>,
        character: Option<String>,
        game: Option<String>,
        save_password: bool,
        profile_name: Option<String>,
        /// Set (both) for a Lich attach instead of a direct eAccess login.
        lich_host: Option<String>,
        lich_port: Option<u16>,
    },
    /// End the session and suppress reconnection (headless runtime only).
    Disconnect,
    /// Request the saved-profile list (direct `profiles` reply).
    GetProfiles,
    /// Delete a saved profile (and its stored password if unshared).
    DeleteProfile { name: String },
    /// Read a whitelisted config file (settings sheet editor).
    ConfigGet { request_id: u64, file: String },
    /// Validate + write a whitelisted config file, then hot-reload.
    ConfigPut {
        request_id: u64,
        file: String,
        content: String,
    },
    /// Structured highlight-rule list for the editor UI.
    HighlightsGet { request_id: u64, scope: String },
    /// Create/update one highlight rule by name.
    HighlightPut {
        request_id: u64,
        scope: String,
        name: String,
        rule: serde_json::Value,
    },
    /// Delete one highlight rule by name.
    HighlightDelete {
        request_id: u64,
        scope: String,
        name: String,
    },
    /// Structured color config for the editor UI.
    ColorsGet { request_id: u64, scope: String },
    /// Validate + write the full color config, then hot-reload.
    ColorsPut {
        request_id: u64,
        scope: String,
        colors: serde_json::Value,
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
        "connect" => {
            let profile = opt_str(msg.d.get("profile"));
            let account = opt_str(msg.d.get("account"));
            let character = opt_str(msg.d.get("character"));
            let lich = msg.d.get("mode").and_then(|v| v.as_str()) == Some("lich");
            let lich_host = lich.then(|| opt_str(msg.d.get("host"))).flatten();
            // Port may arrive as a number or as raw input-field text.
            let lich_port = lich
                .then(|| match msg.d.get("port") {
                    Some(v) if v.is_u64() => v.as_u64().and_then(|p| u16::try_from(p).ok()),
                    Some(v) => v.as_str().and_then(|s| s.trim().parse::<u16>().ok()),
                    None => None,
                })
                .flatten();
            // A connect needs a saved profile, direct credentials, or a
            // complete Lich target.
            if lich {
                if profile.is_none() && (lich_host.is_none() || lich_port.is_none()) {
                    return None;
                }
            } else if profile.is_none() && (account.is_none() || character.is_none()) {
                return None;
            }
            Some(ClientMessage::Connect {
                profile,
                account,
                // Password may legitimately contain leading/trailing spaces;
                // don't trim, only reject empty.
                password: msg
                    .d
                    .get("password")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                character,
                game: opt_str(msg.d.get("game")),
                save_password: msg
                    .d
                    .get("save_password")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                profile_name: opt_str(msg.d.get("profile_name")),
                lich_host,
                lich_port,
            })
        }
        "disconnect" => Some(ClientMessage::Disconnect),
        "get_profiles" => Some(ClientMessage::GetProfiles),
        "config_get" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let file = msg.d.get("file")?.as_str()?.to_string();
            Some(ClientMessage::ConfigGet { request_id, file })
        }
        "config_put" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let file = msg.d.get("file")?.as_str()?.to_string();
            let content = msg.d.get("content")?.as_str()?.to_string();
            Some(ClientMessage::ConfigPut {
                request_id,
                file,
                content,
            })
        }
        "highlights_get" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            Some(ClientMessage::HighlightsGet { request_id, scope })
        }
        "highlight_put" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            let name = msg.d.get("name")?.as_str()?.to_string();
            let rule = msg.d.get("rule")?.clone();
            if !rule.is_object() {
                return None;
            }
            Some(ClientMessage::HighlightPut {
                request_id,
                scope,
                name,
                rule,
            })
        }
        "colors_get" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            Some(ClientMessage::ColorsGet { request_id, scope })
        }
        "colors_put" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            let colors = msg.d.get("colors")?.clone();
            if !colors.is_object() {
                return None;
            }
            Some(ClientMessage::ColorsPut {
                request_id,
                scope,
                colors,
            })
        }
        "highlight_delete" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            let name = msg.d.get("name")?.as_str()?.to_string();
            Some(ClientMessage::HighlightDelete {
                request_id,
                scope,
                name,
            })
        }
        "delete_profile" => {
            let name = msg.d.get("name")?.as_str()?.to_string();
            Some(ClientMessage::DeleteProfile { name })
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
            timestamp: None,
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
    fn parse_session_control_messages() {
        // Saved-profile connect (password optional).
        assert_eq!(
            parse_client_message(r#"{"t":"connect","d":{"profile":"Main"}}"#),
            Some(ClientMessage::Connect {
                profile: Some("Main".to_string()),
                account: None,
                password: None,
                character: None,
                game: None,
                save_password: false,
                profile_name: None,
                lich_host: None,
                lich_port: None,
            })
        );
        // Inline credentials with save.
        assert_eq!(
            parse_client_message(
                r#"{"t":"connect","d":{"account":"ACCT","password":"p w","character":"Testy","game":"prime","save_password":true,"profile_name":"Testy"}}"#
            ),
            Some(ClientMessage::Connect {
                profile: None,
                account: Some("ACCT".to_string()),
                password: Some("p w".to_string()),
                character: Some("Testy".to_string()),
                game: Some("prime".to_string()),
                save_password: true,
                profile_name: Some("Testy".to_string()),
                lich_host: None,
                lich_port: None,
            })
        );
        // Neither a profile nor complete inline credentials → rejected.
        assert_eq!(
            parse_client_message(r#"{"t":"connect","d":{"account":"ACCT"}}"#),
            None
        );
        // Lich attach: host + port, no credentials. Port accepted as a
        // number or as raw input-field text.
        for port_json in [r#""port":8000"#, r#""port":"8000""#] {
            assert_eq!(
                parse_client_message(&format!(
                    r#"{{"t":"connect","d":{{"mode":"lich","host":"100.64.0.7","name":"Testy","character":"Testy",{port_json}}}}}"#
                )),
                Some(ClientMessage::Connect {
                    profile: None,
                    account: None,
                    password: None,
                    character: Some("Testy".to_string()),
                    game: None,
                    save_password: false,
                    profile_name: None,
                    lich_host: Some("100.64.0.7".to_string()),
                    lich_port: Some(8000),
                })
            );
        }
        // Lich mode without a complete target or profile → rejected.
        assert_eq!(
            parse_client_message(r#"{"t":"connect","d":{"mode":"lich","host":"pc.local"}}"#),
            None
        );
        // Lich mode by saved profile name alone is fine.
        assert!(matches!(
            parse_client_message(r#"{"t":"connect","d":{"mode":"lich","profile":"Home"}}"#),
            Some(ClientMessage::Connect { profile: Some(_), .. })
        ));
        assert_eq!(
            parse_client_message(r#"{"t":"disconnect","d":{}}"#),
            Some(ClientMessage::Disconnect)
        );
        assert_eq!(
            parse_client_message(r#"{"t":"get_profiles","d":{}}"#),
            Some(ClientMessage::GetProfiles)
        );
        assert_eq!(
            parse_client_message(r#"{"t":"delete_profile","d":{"name":"Main"}}"#),
            Some(ClientMessage::DeleteProfile {
                name: "Main".to_string()
            })
        );
    }

    #[test]
    fn session_delta_and_snapshot_field() {
        use crate::core::remote::{RemoteSessionInfo, SessionState};
        let info = RemoteSessionInfo {
            state: SessionState::Reconnecting,
            character: Some("Testy".to_string()),
            game: None,
            attempt: Some(3),
            error: None,
            session_control: true,
        };
        let json: serde_json::Value =
            serde_json::from_str(&delta(&RemoteDelta::Session(info.clone()), 5)).unwrap();
        assert_eq!(json["t"], "session");
        assert_eq!(json["d"]["state"], "reconnecting");
        assert_eq!(json["d"]["attempt"], 3);
        assert_eq!(json["d"]["session_control"], true);

        let mut state = RemoteStateSnapshot::default();
        state.session = info;
        let json: serde_json::Value =
            serde_json::from_str(&snapshot(&state, Vec::new(), SnapshotMode::Full, 0)).unwrap();
        assert_eq!(json["d"]["session"]["state"], "reconnecting");
        assert_eq!(json["d"]["session"]["character"], "Testy");
    }

    #[test]
    fn parse_config_editor_messages() {
        assert_eq!(
            parse_client_message(r#"{"t":"config_get","d":{"request_id":7,"file":"highlights"}}"#),
            Some(ClientMessage::ConfigGet {
                request_id: 7,
                file: "highlights".to_string()
            })
        );
        assert_eq!(
            parse_client_message(
                r#"{"t":"config_put","d":{"request_id":8,"file":"colors","content":"[presets]"}}"#
            ),
            Some(ClientMessage::ConfigPut {
                request_id: 8,
                file: "colors".to_string(),
                content: "[presets]".to_string()
            })
        );
        // Missing content → rejected.
        assert_eq!(
            parse_client_message(r#"{"t":"config_put","d":{"request_id":8,"file":"colors"}}"#),
            None
        );
    }

    #[test]
    fn config_file_delta_shape() {
        let d = RemoteDelta::ConfigFile {
            client_id: 3,
            request_id: 9,
            file: "highlights".to_string(),
            content: None,
            error: Some("Invalid TOML: boom".to_string()),
            saved: false,
        };
        let json: serde_json::Value = serde_json::from_str(&delta(&d, 1)).unwrap();
        assert_eq!(json["t"], "config_file");
        assert_eq!(json["d"]["request_id"], 9);
        assert_eq!(json["d"]["error"], "Invalid TOML: boom");
        assert_eq!(json["d"]["saved"], false);
        // client_id stays server-side.
        assert!(json["d"].get("client_id").is_none());
    }

    #[test]
    fn profiles_reply_masks_accounts() {
        assert_eq!(mask_account("MYACCOUNT"), "MY*******");
        assert_eq!(mask_account("ab"), "ab");
        assert_eq!(mask_account("a"), "a");
        let list = vec![
            ProfileEntry {
                name: "Main".to_string(),
                mode: "direct".to_string(),
                account_masked: mask_account("MYACCOUNT"),
                character: "Testy".to_string(),
                game: "prime".to_string(),
                has_password: true,
                host: None,
                port: None,
            },
            ProfileEntry {
                name: "Home Lich".to_string(),
                mode: "lich".to_string(),
                account_masked: String::new(),
                character: "Testy".to_string(),
                game: String::new(),
                has_password: false,
                host: Some("100.64.0.7".to_string()),
                port: Some(8000),
            },
        ];
        let json: serde_json::Value = serde_json::from_str(&profiles(&list, 9)).unwrap();
        assert_eq!(json["t"], "profiles");
        assert_eq!(json["d"]["list"][0]["account_masked"], "MY*******");
        assert_eq!(json["d"]["list"][0]["has_password"], true);
        assert_eq!(json["d"]["list"][0]["mode"], "direct");
        // Direct entries omit the Lich target fields entirely.
        assert!(json["d"]["list"][0].get("host").is_none());
        assert_eq!(json["d"]["list"][1]["mode"], "lich");
        assert_eq!(json["d"]["list"][1]["host"], "100.64.0.7");
        assert_eq!(json["d"]["list"][1]["port"], 8000);
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
                timestamp: None,
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
