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
    RemoteCharInfo, RemoteDelta, RemoteInventoryTree, RemoteMacros, RemoteMenuItem,
    RemoteRoomEntities, RemoteSessionInfo, RemoteStateSnapshot, RemoteTarget, RemoteWheels,
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
    /// Room description prose as styled lines (color + scenery links);
    /// empty when unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    description: Vec<crate::data::widget::StyledLine>,
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
    /// Absolute vitals; omitted until the minivitals dialog reports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    minivitals: Vec<crate::core::remote::RemoteVital>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prepared_spell: Option<String>,
    /// Group roster. Omitted when not grouped, so existing clients that do
    /// not read it see no change on the wire.
    #[serde(default, skip_serializing_if = "group_is_empty")]
    group: crate::core::group::GroupState,
    rt: RtPayload,
    effects: Vec<ActiveEffectsContent>,
    #[serde(default, skip_serializing_if = "objectives_is_empty")]
    objectives: crate::data::ObjectivesContent,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    spellbook: Vec<crate::data::widget::StyledLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    inventory: Vec<crate::data::widget::StyledLine>,
    /// Distinguishes a received empty inventory snapshot from startup state.
    #[serde(default, skip_serializing_if = "bool_is_false")]
    inventory_received: bool,
    /// Structured inventory-manager graph. This is additive to the rendered
    /// inventory lines and absent until the core has a manager snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inventory_tree: Option<RemoteInventoryTree>,
    injuries: std::collections::HashMap<String, u8>,
    /// Active doll variant + suppressed parts (host-resolved skin rules).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    doll_variant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    doll_hidden: Vec<String>,
    targets: Vec<RemoteTarget>,
    /// Creature-field cards, host-placed on the 880x470 virtual stage in
    /// draw order (see RemoteFieldCard). Kept in watch snapshots — the
    /// /creatures page is a watch client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    field: Vec<crate::core::remote::RemoteFieldCard>,
    entities: RemoteRoomEntities,
    portals: Vec<String>,
    char_info: RemoteCharInfo,
    session: RemoteSessionInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    webui_pages: Vec<crate::data::webui::WebUiPageDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    map_scene: Option<Arc<crate::core::remote::RemoteMapScene>>,
    map_state: crate::core::remote::RemoteMapState,
    /// Scrollback. Skipped when empty so a watch snapshot does not carry an
    /// empty array; the phone always has lines, so its wire is unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    text: Vec<SnapshotLine>,
}

/// An ungrouped character ships no group object at all, so clients that do
/// not read it (the phone) see an unchanged wire.
fn group_is_empty(group: &crate::core::group::GroupState) -> bool {
    !group.is_grouped()
}

/// A character with no quest feed ships no objectives object at all, so
/// clients that do not read it see an unchanged wire.
fn objectives_is_empty(objectives: &crate::data::ObjectivesContent) -> bool {
    objectives.objectives.is_empty()
}

fn bool_is_false(value: &bool) -> bool {
    !*value
}

/// First message on every connection.
pub fn hello(character: Option<String>, streams: Vec<String>, session: String, seq: u64) -> String {
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
    snapshot_for(state, lines, mode, seq, SubscribeMode::Play)
}

/// Snapshot tailored to what the client is here for.
///
/// A `Watch` client gets status only. That matters at connect: the text
/// scrollback is 300 lines PER STREAM, so a dozen live streams is a few
/// thousand styled lines in one frame, and `map_scene` can be thousands of
/// rooms on top. Paying that once per sibling connection -- for a display
/// that renders no text and draws no map -- is the whole reason this mode
/// exists.
pub fn snapshot_for(
    state: &RemoteStateSnapshot,
    lines: Vec<RemoteLine>,
    mode: SnapshotMode,
    seq: u64,
    sub: SubscribeMode,
) -> String {
    let mut payload = SnapshotPayload {
        mode,
        character: state.character.clone(),
        vitals: state.vitals.clone(),
        room: RoomPayload {
            name: state.room_name.clone(),
            exits: state.exits.clone(),
            id: state.room_id.clone(),
            description: state.room_description.clone(),
        },
        hands: HandsPayload {
            left: state.left_hand.clone(),
            right: state.right_hand.clone(),
        },
        indicators: state.indicators.clone(),
        minivitals: state.minivitals.clone(),
        prepared_spell: state.prepared_spell.clone(),
        group: state.group.clone(),
        rt: RtPayload {
            roundtime_end: state.roundtime_end,
            casttime_end: state.casttime_end,
            server_time: state.server_time,
        },
        effects: state.effects.clone(),
        objectives: state.objectives.clone(),
        spellbook: state.spellbook.clone(),
        inventory: state.inventory.clone(),
        inventory_received: state.inventory_received,
        inventory_tree: state.inventory_tree.clone(),
        injuries: state.injuries.clone(),
        doll_variant: state.doll_variant.clone(),
        doll_hidden: state.doll_hidden.clone(),
        targets: state.targets.clone(),
        field: state.field.clone(),
        entities: state.entities.clone(),
        portals: state.portals.clone(),
        char_info: state.char_info.clone(),
        session: state.session.clone(),
        webui_pages: state.webui_pages.clone(),
        map_scene: state.map_scene.0.clone(),
        map_state: state.map_state.clone(),
        text: lines
            .into_iter()
            .map(|l| SnapshotLine {
                seq: l.seq,
                stream: l.stream,
                line: l.line,
            })
            .collect(),
    };
    if sub == SubscribeMode::Watch {
        payload.strip_for_watch();
    }
    encode("snapshot", seq, payload)
}

impl SnapshotPayload {
    /// Everything a Watch client does not pay for, in ONE place.
    ///
    /// The old shape was nine inline `if watching` ternaries inside the
    /// struct literal, which meant every FUTURE payload field shipped to
    /// watchers by default and invisibly -- with six sibling instances, six
    /// copies of it per connect. `watch_snapshot_key_allowlist` in the tests
    /// fails on any new field until it is classified here or there.
    fn strip_for_watch(&mut self) {
        // Room identity (name + id) stays: it drives the "not with you" cue.
        self.room.exits = Vec::new();
        self.room.description = Vec::new();
        self.spellbook = Vec::new();
        self.inventory = Vec::new();
        self.inventory_received = false;
        self.inventory_tree = None;
        self.targets = Vec::new();
        self.entities = Default::default();
        self.portals = Vec::new();
        self.webui_pages = Vec::new();
        self.map_scene = None;
        self.map_state = Default::default();
        self.text = Vec::new();
    }
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
        RemoteDelta::Room {
            name,
            exits,
            id,
            description,
        } => encode(
            "room",
            last_seq,
            RoomPayload {
                name: name.clone(),
                exits: exits.clone(),
                id: id.clone(),
                description: description.clone(),
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
        RemoteDelta::Group(group) => encode("group", last_seq, group.clone()),
        RemoteDelta::MiniVitals(vitals) => encode("minivitals", last_seq, vitals.clone()),
        RemoteDelta::PreparedSpell(spell) => encode(
            "prepared_spell",
            last_seq,
            serde_json::json!({ "spell": spell }),
        ),
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
        // client_id stays server-side: the ws task already filtered on it.
        RemoteDelta::OpenUrl { url, .. } => encode(
            "open_url",
            last_seq,
            serde_json::json!({ "url": url }),
        ),
        RemoteDelta::Macros(m) => macros(m, last_seq),
        RemoteDelta::Wheels(w) => wheels(w, last_seq),
        RemoteDelta::Effects(effects) => encode("effects", last_seq, effects),
        RemoteDelta::Objectives(objectives) => encode("objectives", last_seq, objectives),
        RemoteDelta::Spells(lines) => encode("spells", last_seq, lines),
        RemoteDelta::Inventory(lines) => encode("inventory", last_seq, lines),
        RemoteDelta::InventoryReceived(received) => {
            encode("inventory_received", last_seq, received)
        }
        RemoteDelta::InventoryTree(tree) => encode("inventory_tree", last_seq, tree),
        RemoteDelta::Session(info) => encode("session", last_seq, info),
        RemoteDelta::Injuries(injuries) => encode("injuries", last_seq, injuries),
        RemoteDelta::Doll { variant, hidden } => encode(
            "doll",
            last_seq,
            serde_json::json!({ "variant": variant, "hidden": hidden }),
        ),
        RemoteDelta::Targets(targets) => encode("targets", last_seq, targets),
        RemoteDelta::Field(cards) => encode("field", last_seq, cards),
        RemoteDelta::Entities(entities) => encode("entities", last_seq, entities),
        RemoteDelta::Portals(portals) => encode("portals", last_seq, portals),
        RemoteDelta::CharInfo(info) => encode("charinfo", last_seq, info),
        RemoteDelta::Sound { file, volume } => encode(
            "sound",
            last_seq,
            serde_json::json!({ "file": file, "volume": volume }),
        ),
        RemoteDelta::MapScene(scene) => encode("map_scene", last_seq, scene),
        RemoteDelta::MapState(state) => encode("map_state", last_seq, state),
        // client_id stays server-side (ws task already filtered on it).
        RemoteDelta::MapLocations {
            request_id,
            locations,
            ..
        } => encode(
            "map_locations",
            last_seq,
            serde_json::json!({ "request_id": request_id, "locations": locations }),
        ),
        RemoteDelta::MapBrowse {
            request_id,
            location,
            scene,
            error,
            ..
        } => encode(
            "map_browse",
            last_seq,
            serde_json::json!({
                "request_id": request_id,
                "location": location,
                "scene": scene,
                "error": error,
            }),
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
        RemoteDelta::TouchWheel {
            request_id,
            scope,
            slices,
            catalog,
            error,
            saved,
            ..
        } => encode(
            "touch_wheel",
            last_seq,
            serde_json::json!({
                "request_id": request_id,
                "scope": scope,
                "slices": slices,
                "catalog": catalog,
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
                // The canonical highlight-field catalog: the phone renders its
                // form from this so a field added to HIGHLIGHT_FIELDS surfaces
                // on the phone without a client edit, and can never silently
                // drift from the desktop's field set.
                "fields": crate::config::highlight_web_fields(),
            }),
        ),
        RemoteDelta::Settings {
            request_id,
            catalog,
            key,
            error,
            saved,
            ..
        } => encode(
            "settings",
            last_seq,
            serde_json::json!({
                "request_id": request_id,
                "catalog": catalog,
                "key": key,
                "error": error,
                "saved": saved,
            }),
        ),
        // The catalog object's fields (streams/windows/fallback) ride at
        // the payload top level, per-request fields alongside them.
        RemoteDelta::Streams {
            request_id,
            data,
            stream,
            error,
            saved,
            ..
        } => {
            let mut payload = match data {
                serde_json::Value::Object(map) => map.clone(),
                _ => serde_json::Map::new(),
            };
            payload.insert("request_id".to_string(), serde_json::json!(request_id));
            payload.insert("stream".to_string(), serde_json::json!(stream));
            payload.insert("error".to_string(), serde_json::json!(error));
            payload.insert("saved".to_string(), serde_json::json!(saved));
            encode("streams", last_seq, serde_json::Value::Object(payload))
        }
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
        // client_id stays server-side (ws task already filtered on it).
        RemoteDelta::LauncherSsh {
            request_id,
            settings,
            public_key,
            error,
            saved,
            ..
        } => encode(
            "launcher_ssh",
            last_seq,
            serde_json::json!({
                "request_id": request_id,
                "settings": settings,
                "public_key": public_key,
                "error": error,
                "saved": saved,
            }),
        ),
        // Lich WebUI broadcasts. The phone renders only pages it subscribed
        // to; it drops renders for pages it hasn't opened.
        RemoteDelta::WebUiRender { page, seq, tree } => encode(
            "webui_render",
            last_seq,
            serde_json::json!({ "page": page, "seq": seq, "tree": tree }),
        ),
        RemoteDelta::WebUiPages(pages) => encode(
            "webui_pages",
            last_seq,
            serde_json::json!({ "pages": pages }),
        ),
        RemoteDelta::WebUiPageClosed { page } => encode(
            "webui_closed",
            last_seq,
            serde_json::json!({ "page": page }),
        ),
        RemoteDelta::WebUiNotice { level, text } => encode(
            "webui_notice",
            last_seq,
            serde_json::json!({ "level": level, "text": text }),
        ),
        RemoteDelta::WebUiConnected { connected } => encode(
            "webui_connected",
            last_seq,
            serde_json::json!({ "connected": connected }),
        ),
        RemoteDelta::SkillTrainer { open, status, data } => encode(
            "skill_trainer",
            last_seq,
            serde_json::json!({ "open": open, "status": status, "data": data }),
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
    /// The Lich launch command, if this profile has one (mobile cold-start).
    /// Present tells the client this saved login will SSH-launch on connect
    /// if the port is down; the client shows it in the edit form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_launch: Option<String>,
}

/// Saved-profile list; direct reply to a `get_profiles` request.
pub fn profiles(list: &[ProfileEntry], seq: u64) -> String {
    encode("profiles", seq, serde_json::json!({ "list": list }))
}

/// Mask an account name for display: first two characters + asterisks.
pub fn mask_account(account: &str) -> String {
    let visible: String = account.chars().take(2).collect();
    format!(
        "{visible}{}",
        "*".repeat(account.chars().count().saturating_sub(2))
    )
}

/// Macro definitions; sent on connect and after `.reloadmacros`.
pub fn macros(m: &RemoteMacros, seq: u64) -> String {
    encode("macros", seq, m)
}

/// Radial-wheel definitions; sent on connect and after the wheel config
/// changes (keybinds reload, desktop wheel editor).
pub fn wheels(w: &RemoteWheels, seq: u64) -> String {
    encode("wheels", seq, w)
}

/// Sent right before closing an unauthenticated connection, so the client
/// can show its pairing prompt instead of retry-looping.
pub fn denied() -> String {
    encode("denied", 0, serde_json::json!({}))
}

/// What a connected client is here to do.
///
/// These are different jobs, not a volume knob. `Play` and `Desktop` both
/// receive the full feed; the distinct desktop value identifies Despana
/// without coupling browser-tab lifetime to game-session lifetime. `Watch` is
/// a status observer -- the multi-account display -- which never renders a
/// line of game text and would otherwise pay for the full feed once per
/// sibling connection.
///
/// `Play` is the default precisely because a client that does not ask is the
/// phone, which shipped before this existed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubscribeMode {
    /// Everything. The default.
    #[default]
    Play,
    /// Everything, tagged as the full Despana presentation.
    Desktop,
    /// Status only: no text scrollback, no map, no room prose, no spellbook.
    Watch,
}

impl SubscribeMode {
    fn from_wire(s: &str) -> Self {
        match s {
            "watch" => Self::Watch,
            "desktop" => Self::Desktop,
            // Unknown modes fall back to the full feed rather than silently
            // starving a client of the data it came for.
            _ => Self::Play,
        }
    }

    /// Whether a delta is worth sending to a client in this mode.
    ///
    /// Watchers get the status set and nothing else. Addressed request/reply
    /// deltas are filtered separately by client id, so they are not listed.
    pub fn wants(&self, delta: &crate::core::remote::RemoteDelta) -> bool {
        use crate::core::remote::RemoteDelta as D;
        match self {
            Self::Play | Self::Desktop => true,
            Self::Watch => matches!(
                delta,
                D::Vitals(_)
                    // Room identity drives the card's "not with you" cue; the
                    // server slims the prose out of the delta for watchers
                    // before encoding, so this ships name + id, not the
                    // description.
                    | D::Room { .. }
                    | D::MiniVitals(_)
                    | D::PreparedSpell(_)
                    | D::Indicators(_)
                    | D::Group(_)
                    | D::Rt { .. }
                    | D::Injuries(_)
                    | D::CharInfo(_)
                    | D::Effects(_)
                    | D::Objectives(_)
                    | D::Hands { .. }
                    | D::Doll { .. }
                    | D::Field(_)
                    | D::Session(_)
            ),
        }
    }
}

/// Messages a client may send. Unknown types are ignored (forward compat).
#[derive(Debug, PartialEq)]
pub enum ClientMessage {
    /// Pairing token; must be the first message on every connection.
    Auth { token: String },
    /// Declare what this connection is for. Optional; absent means `Play`,
    /// which is what every pre-existing client implies.
    Subscribe { mode: SubscribeMode },
    /// Explicit Despana request to quit the game normally and terminate the
    /// owning headless runtime after the game transport disconnects.
    ExitLogout,
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
    /// server-side (the client never sends macro command text). Type-in
    /// (`insert`) buttons never arrive here — the client handles them
    /// locally.
    Macro { id: String },
    /// A radial-wheel slice picked (wheel button released or South on a
    /// leaf). `key` is "" for the default wheel, else a named wheel;
    /// `path` indexes down to the leaf. Resolved to its command
    /// server-side, like macros.
    WheelPick { key: String, path: Vec<usize> },
    /// Create/edit a phone-authored macro button (macros-local.toml).
    MacroSave {
        group: Option<String>,
        label: String,
        command: String,
        color: Option<String>,
        confirm: bool,
        insert: bool,
        /// Client-side action (wheel-slice vocabulary); wins over
        /// `command` and never resolves server-side.
        client: Option<String>,
        options: Vec<crate::config::MacroOption>,
        original: Option<(Option<String>, String)>,
    },
    /// Delete a phone-authored macro button.
    MacroDelete {
        group: Option<String>,
        label: String,
    },
    /// The map location picker wants the list of mapped locations.
    MapLocations { request_id: u64 },
    /// Browse another location's map.
    MapView { request_id: u64, location: String },
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
        /// Lich launch command (mobile cold-start): SSH-launch if the port is
        /// down before attaching. Only meaningful with a Lich target.
        custom_launch: Option<String>,
    },
    /// End the session and suppress reconnection (headless runtime only).
    Disconnect,
    /// Request the saved-profile list (direct `profiles` reply).
    GetProfiles,
    /// Delete a saved profile (and its stored password if unshared).
    DeleteProfile { name: String },
    /// Read the SSH-launcher settings (user/host/port/OS + key state).
    LauncherSshGet { request_id: u64 },
    /// Write the SSH-launcher settings; `generate_key` mints a fresh key.
    LauncherSshPut {
        request_id: u64,
        user: String,
        host: String,
        port: u16,
        remote_os: String,
        generate_key: bool,
    },
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
    /// The full settings catalog (registry dump + live values).
    SettingsGet { request_id: u64 },
    /// Set one registered setting: JSON value typed by the setting's kind,
    /// scope "character" or "global". `clear` resets a sensitive
    /// optional-text setting to None (its redacted value never crossed the
    /// wire, so the client can't send it back emptied).
    SettingsPut {
        request_id: u64,
        key: String,
        value: serde_json::Value,
        scope: String,
        clear: bool,
    },
    /// The streams catalog (every known stream + where it goes).
    StreamsGet { request_id: u64 },
    /// Set one stream's orphan route: target "discard" | "main" |
    /// "window:<name>" | "clear" (reset to fallback). Route editing only —
    /// window subscriptions are read-only from the phone.
    StreamsPut {
        request_id: u64,
        stream: String,
        target: String,
    },
    /// Structured color config for the editor UI.
    ColorsGet { request_id: u64, scope: String },
    /// Validate + write the full color config, then hot-reload.
    ColorsPut {
        request_id: u64,
        scope: String,
        colors: serde_json::Value,
    },
    /// The touch wheel's slice list + the client-action vocabulary catalog,
    /// for the phone's wheel editor.
    TouchWheelGet { request_id: u64, scope: String },
    /// Validate + write the touch wheel's slice list, then hot-reload and
    /// re-broadcast the `wheels` message so it applies live.
    TouchWheelPut {
        request_id: u64,
        scope: String,
        slices: serde_json::Value,
    },
    /// The phone opened a Lich WebUI panel: subscribe to the page so renders
    /// flow. Core forwards a `subscribe` to Lich.
    WebUiSubscribe { page: String },
    /// The phone closed a WebUI panel: unsubscribe.
    WebUiUnsubscribe { page: String },
    /// A phone WebUI interaction (button/input/row); core forwards it to Lich.
    WebUiEvent {
        page: String,
        cid: String,
        value: serde_json::Value,
    },
    /// Open the skill-trainer panel (fetches `goals` if nothing loaded yet).
    SkillTrainerOpen,
    /// Step one skill's goal by `n` (the 1/10/100 +/- buttons).
    SkillTrainerStep { id: u32, n: u32, raise: bool },
    /// Submit the current goals to play.net.
    SkillTrainerApply,
    /// Re-fetch a fresh skill-manager page.
    SkillTrainerReload,
    /// Save the current goals as a named per-character profile.
    SkillTrainerProfileSave { name: String },
    /// Load a named profile into the editor.
    SkillTrainerProfileLoad { name: String },
    /// Delete a named profile.
    SkillTrainerProfileDelete { name: String },
}

fn opt_str(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// Trim a phone-authored macro command, preserving a deliberate trailing
/// `\r` on type-in (`insert`) macros — it encodes "type, then send" and a
/// plain trim would eat it.
fn trim_macro_command(raw: &str, insert: bool) -> String {
    let mut command = raw.trim().to_string();
    if insert && !command.is_empty() && raw.trim_end_matches([' ', '\t']).ends_with('\r') {
        command.push('\r');
    }
    command
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
        "subscribe" => {
            // A missing or unrecognized mode means the full feed, so a
            // malformed subscribe degrades to today's behavior rather than
            // leaving a client with no data.
            let mode = msg
                .d
                .get("mode")
                .and_then(|v| v.as_str())
                .map(SubscribeMode::from_wire)
                .unwrap_or_default();
            Some(ClientMessage::Subscribe { mode })
        }
        "exit_logout" => Some(ClientMessage::ExitLogout),
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
        "wheel_pick" => {
            let key = msg
                .d
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let path: Vec<usize> = msg
                .d
                .get("path")?
                .as_array()?
                .iter()
                .map(|v| v.as_u64().map(|i| i as usize))
                .collect::<Option<_>>()?;
            if path.is_empty() {
                return None;
            }
            Some(ClientMessage::WheelPick { key, path })
        }
        "macro_save" => {
            let label = msg.d.get("label")?.as_str()?.trim().to_string();
            let insert = msg
                .d
                .get("insert")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let command = trim_macro_command(
                msg.d
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default(),
                insert,
            );
            let options: Vec<crate::config::MacroOption> = msg
                .d
                .get("options")
                .and_then(|v| v.as_array())
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|o| {
                            let label = o.get("label")?.as_str()?.trim().to_string();
                            let insert = o.get("insert").and_then(|v| v.as_bool()).unwrap_or(false);
                            let command = trim_macro_command(o.get("command")?.as_str()?, insert);
                            if label.is_empty() || command.is_empty() {
                                return None;
                            }
                            Some(crate::config::MacroOption {
                                label,
                                command,
                                confirm: o
                                    .get("confirm")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false),
                                insert,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let client = opt_str(msg.d.get("client"));
            // A button needs a label and either a direct command, a
            // client action, or at least one option (menu button).
            if label.is_empty() || (command.is_empty() && client.is_none() && options.is_empty()) {
                return None;
            }
            let original = msg
                .d
                .get("original")
                .filter(|v| !v.is_null())
                .and_then(|o| {
                    Some((
                        opt_str(o.get("group")),
                        o.get("label")?.as_str()?.to_string(),
                    ))
                });
            Some(ClientMessage::MacroSave {
                group: opt_str(msg.d.get("group")),
                label,
                command,
                color: opt_str(msg.d.get("color")),
                confirm: msg
                    .d
                    .get("confirm")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                insert,
                client,
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
                custom_launch: lich.then(|| opt_str(msg.d.get("custom_launch"))).flatten(),
            })
        }
        "map_locations" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            Some(ClientMessage::MapLocations { request_id })
        }
        "map_view" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let location = msg.d.get("location")?.as_str()?.to_string();
            Some(ClientMessage::MapView {
                request_id,
                location,
            })
        }
        "disconnect" => Some(ClientMessage::Disconnect),
        "get_profiles" => Some(ClientMessage::GetProfiles),
        "launcher_ssh_get" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            Some(ClientMessage::LauncherSshGet { request_id })
        }
        "launcher_ssh_put" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let user = opt_str(msg.d.get("user")).unwrap_or_default();
            let host = opt_str(msg.d.get("host")).unwrap_or_default();
            // Port may arrive as number or text; default to 22.
            let port = match msg.d.get("port") {
                Some(v) if v.is_u64() => v.as_u64().and_then(|p| u16::try_from(p).ok()),
                Some(v) => v.as_str().and_then(|s| s.trim().parse::<u16>().ok()),
                None => None,
            }
            .unwrap_or(22);
            let remote_os =
                opt_str(msg.d.get("remote_os")).unwrap_or_else(|| "windows".to_string());
            let generate_key = msg
                .d
                .get("generate_key")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(ClientMessage::LauncherSshPut {
                request_id,
                user,
                host,
                port,
                remote_os,
                generate_key,
            })
        }
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
        "settings_get" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            Some(ClientMessage::SettingsGet { request_id })
        }
        "settings_put" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let key = msg.d.get("key")?.as_str()?.to_string();
            let value = msg.d.get("value")?.clone();
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            if !matches!(scope.as_str(), "character" | "global") {
                return None;
            }
            Some(ClientMessage::SettingsPut {
                request_id,
                key,
                value,
                scope,
                clear: msg
                    .d
                    .get("clear")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            })
        }
        "streams_get" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            Some(ClientMessage::StreamsGet { request_id })
        }
        "streams_put" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let stream = msg.d.get("stream")?.as_str()?.to_string();
            let target = msg.d.get("target")?.as_str()?.to_string();
            Some(ClientMessage::StreamsPut {
                request_id,
                stream,
                target,
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
        "touch_wheel_get" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            Some(ClientMessage::TouchWheelGet { request_id, scope })
        }
        "touch_wheel_put" => {
            let request_id = msg.d.get("request_id")?.as_u64()?;
            let scope = msg.d.get("scope")?.as_str()?.to_string();
            let slices = msg.d.get("slices")?.clone();
            if !slices.is_array() {
                return None;
            }
            Some(ClientMessage::TouchWheelPut {
                request_id,
                scope,
                slices,
            })
        }
        "webui_subscribe" => {
            let page = msg.d.get("page")?.as_str()?.to_string();
            Some(ClientMessage::WebUiSubscribe { page })
        }
        "webui_unsubscribe" => {
            let page = msg.d.get("page")?.as_str()?.to_string();
            Some(ClientMessage::WebUiUnsubscribe { page })
        }
        "webui_event" => {
            let page = msg.d.get("page")?.as_str()?.to_string();
            let cid = msg.d.get("cid")?.as_str()?.to_string();
            // value is component-specific; null is valid (button clicks).
            let value = msg
                .d
                .get("value")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Some(ClientMessage::WebUiEvent { page, cid, value })
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
        "skill_trainer_open" => Some(ClientMessage::SkillTrainerOpen),
        "skill_trainer_reload" => Some(ClientMessage::SkillTrainerReload),
        "skill_trainer_apply" => Some(ClientMessage::SkillTrainerApply),
        "skill_trainer_step" => {
            let id = u32::try_from(msg.d.get("id")?.as_u64()?).ok()?;
            let n = u32::try_from(msg.d.get("n")?.as_u64()?).ok()?;
            let raise = msg.d.get("raise").and_then(|v| v.as_bool()).unwrap_or(true);
            Some(ClientMessage::SkillTrainerStep { id, n, raise })
        }
        "skill_trainer_profile_save" => {
            let name = msg.d.get("name")?.as_str()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            Some(ClientMessage::SkillTrainerProfileSave { name })
        }
        "skill_trainer_profile_load" => {
            let name = msg.d.get("name")?.as_str()?.to_string();
            Some(ClientMessage::SkillTrainerProfileLoad { name })
        }
        "skill_trainer_profile_delete" => {
            let name = msg.d.get("name")?.as_str()?.to_string();
            Some(ClientMessage::SkillTrainerProfileDelete { name })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::TextSegment;

    fn snap_json(sub: SubscribeMode, state: &RemoteStateSnapshot) -> serde_json::Value {
        let lines = vec![RemoteLine {
            seq: 1,
            stream: "main".to_string(),
            line: Arc::new(crate::data::widget::StyledLine {
                segments: vec![TextSegment::plain("You see a rock.")],
                stream: "main".to_string(),
                timestamp: None,
            }),
        }];
        let raw = snapshot_for(state, lines, SnapshotMode::Full, 1, sub);
        let v: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
        v["d"].clone()
    }

    fn inventory_tree() -> RemoteInventoryTree {
        RemoteInventoryTree {
            room: "2005".to_string(),
            items: vec![
                crate::core::remote::RemoteInventoryItem {
                    id: "bag".to_string(),
                    relation: "worn".to_string(),
                    parent: "player".to_string(),
                    name: "a patchwork backpack".to_string(),
                    article: "a".to_string(),
                    adjective: "patchwork".to_string(),
                    noun: "backpack".to_string(),
                    weight: 5,
                    in_max: Some(2000),
                    flags: vec!["closed".to_string()],
                    ..Default::default()
                },
                crate::core::remote::RemoteInventoryItem {
                    id: "pouch".to_string(),
                    relation: "in".to_string(),
                    parent: "bag".to_string(),
                    name: "a silk pouch".to_string(),
                    article: "a".to_string(),
                    adjective: "silk".to_string(),
                    noun: "pouch".to_string(),
                    weight: 1,
                    in_max: Some(505),
                    flags: vec!["closed".to_string(), "locked".to_string()],
                    ..Default::default()
                },
            ],
            complete: true,
            generation: 7,
        }
    }

    /// The mechanical guard behind strip_for_watch: a watch snapshot built
    /// from a FULLY populated state must serialize only allowlisted keys.
    /// Adding a field to SnapshotPayload fails this test until the field is
    /// classified -- either stripped for watchers or added here on purpose.
    /// Without it, every new payload field shipped to watchers by default,
    /// invisibly, times one copy per sibling instance per connect.
    #[test]
    fn watch_snapshot_key_allowlist() {
        let mut state = RemoteStateSnapshot::default();
        // Populate every bulk field so a leak cannot hide behind
        // skip_serializing_if on an empty default.
        state.room_name = Some("Town Square".to_string());
        state.room_id = Some("1".to_string());
        state.exits = vec!["north".to_string()];
        state.room_description = vec![crate::data::widget::StyledLine {
            segments: vec![TextSegment::plain("prose")],
            stream: "main".to_string(),
            timestamp: None,
        }];
        state.spellbook = state.room_description.clone();
        state.inventory = state.room_description.clone();
        state.inventory_received = true;
        state.inventory_tree = Some(inventory_tree());
        state.portals = vec!["portal".to_string()];
        state.field = vec![crate::core::remote::RemoteFieldCard {
            id: "123".to_string(),
            noun: "kobold".to_string(),
            name: "a kobold".to_string(),
            ..Default::default()
        }];
        state.webui_pages = Vec::new();
        state.prepared_spell = Some("Spirit Warding I".to_string());

        let lines = vec![RemoteLine {
            seq: 1,
            stream: "main".to_string(),
            line: Arc::new(crate::data::widget::StyledLine {
                segments: vec![TextSegment::plain("scrollback")],
                stream: "main".to_string(),
                timestamp: None,
            }),
        }];
        let raw = snapshot_for(&state, lines, SnapshotMode::Full, 1, SubscribeMode::Watch);
        let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
        let allowed = [
            "mode",
            "character",
            "vitals",
            "room",
            "hands",
            "indicators",
            "minivitals",
            "prepared_spell",
            "group",
            "rt",
            "effects",
            "injuries",
            "doll_variant",
            "doll_hidden",
            "targets",
            // Deliberate: the /creatures page is a watch client; cards are
            // small (no art on the wire) and are its whole reason to exist.
            "field",
            "entities",
            "portals",
            "char_info",
            "session",
            "map_state",
        ];
        for key in v["d"].as_object().expect("object").keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "unclassified snapshot field shipped to watchers: {key} --                  strip it in strip_for_watch or allowlist it deliberately"
            );
        }
        // And the stripped bulk stays stripped.
        assert!(v["d"].get("text").is_none());
        assert!(v["d"].get("map_scene").is_none());
        assert!(v["d"]["room"].get("description").is_none());
        assert!(
            v["d"].get("inventory_tree").is_none(),
            "structured inventory is bulk and must not ship to watchers"
        );
        assert!(v["d"].get("inventory_received").is_none());
    }

    #[test]
    fn parse_subscribe_defaults_to_play() {
        assert_eq!(
            parse_client_message(r#"{"t":"subscribe","d":{"mode":"desktop"}}"#),
            Some(ClientMessage::Subscribe {
                mode: SubscribeMode::Desktop
            })
        );
        assert_eq!(
            parse_client_message(r#"{"t":"subscribe","d":{"mode":"watch"}}"#),
            Some(ClientMessage::Subscribe {
                mode: SubscribeMode::Watch
            })
        );
        assert_eq!(
            parse_client_message(r#"{"t":"subscribe","d":{"mode":"play"}}"#),
            Some(ClientMessage::Subscribe {
                mode: SubscribeMode::Play
            })
        );
        // A malformed or unknown mode degrades to the full feed rather than
        // starving the client of the data it came for.
        assert_eq!(
            parse_client_message(r#"{"t":"subscribe","d":{"mode":"nonsense"}}"#),
            Some(ClientMessage::Subscribe {
                mode: SubscribeMode::Play
            })
        );
        assert_eq!(
            parse_client_message(r#"{"t":"subscribe","d":{}}"#),
            Some(ClientMessage::Subscribe {
                mode: SubscribeMode::Play
            })
        );
    }

    #[test]
    fn parses_explicit_exit_logout() {
        assert_eq!(
            parse_client_message(r#"{"t":"exit_logout","d":{}}"#),
            Some(ClientMessage::ExitLogout)
        );
    }

    /// The phone client predates `subscribe`, so a connection that never
    /// sends one must get byte-identical output to before this existed.
    #[test]
    fn play_mode_snapshot_still_carries_everything() {
        let default = snap_json(SubscribeMode::Play, &RemoteStateSnapshot::default());
        assert!(
            default.get("inventory_tree").is_none(),
            "the optional extension must not change snapshots without managed inventory"
        );

        let mut state = RemoteStateSnapshot::default();
        state.inventory_received = true;
        state.inventory_tree = Some(inventory_tree());
        let d = snap_json(SubscribeMode::Play, &state);

        assert!(d.get("text").is_some(), "scrollback must ship");
        assert_eq!(d["text"].as_array().expect("array").len(), 1);
        assert!(d.get("targets").is_some());
        assert!(d.get("entities").is_some());
        assert!(d.get("map_state").is_some());
        assert_eq!(d["inventory_received"], true);
        assert_eq!(d["inventory_tree"]["room"], "2005");
        assert_eq!(d["inventory_tree"]["items"][1]["parent"], "bag");
        assert_eq!(d["inventory_tree"]["items"][1]["in_max"], 505);
        assert_eq!(d["inventory_tree"]["items"][1]["flags"][1], "locked");
        assert!(d["inventory_tree"].get("token").is_none());
    }

    /// A watcher pays for none of the bulk. This is the whole point of the
    /// mode: 300 lines PER STREAM at connect, times one connection per
    /// sibling character, for a display that renders no text.
    #[test]
    fn watch_mode_snapshot_drops_text_and_map() {
        let mut state = RemoteStateSnapshot::default();
        state.inventory_received = true;
        state.inventory_tree = Some(inventory_tree());
        let d = snap_json(SubscribeMode::Watch, &state);

        assert!(d.get("text").is_none(), "scrollback must not ship");
        assert!(d.get("map_scene").is_none(), "map scene must not ship");
        assert!(
            d["spellbook"].as_array().map_or(true, |a| a.is_empty()),
            "spellbook must not ship"
        );
        assert!(
            d["inventory"].as_array().map_or(true, |a| a.is_empty()),
            "inventory must not ship"
        );

        // ...but the status a watcher exists to show is all still there.
        assert!(d.get("vitals").is_some());
        assert!(d.get("indicators").is_some());
        assert!(d.get("injuries").is_some());
        assert!(d.get("rt").is_some());
        assert!(d.get("char_info").is_some());
        assert!(d.get("inventory_tree").is_none());
        assert!(d.get("inventory_received").is_none());
    }

    #[test]
    fn inventory_tree_delta_serializes_replacement_and_null_clear() {
        let replacement = serde_json::from_str::<serde_json::Value>(&delta(
            &RemoteDelta::InventoryTree(Some(inventory_tree())),
            41,
        ))
        .expect("inventory tree replacement json");
        assert_eq!(replacement["t"], "inventory_tree");
        assert_eq!(replacement["seq"], 41);
        assert_eq!(replacement["d"]["generation"], 7);
        assert_eq!(replacement["d"]["items"][1]["relation"], "in");

        let clear = serde_json::from_str::<serde_json::Value>(&delta(
            &RemoteDelta::InventoryTree(None),
            42,
        ))
        .expect("inventory tree clear json");
        assert_eq!(clear["t"], "inventory_tree");
        assert_eq!(clear["seq"], 42);
        assert!(clear["d"].is_null());
    }

    #[test]
    fn inventory_received_delta_serializes_authoritative_empty_state() {
        let message = serde_json::from_str::<serde_json::Value>(&delta(
            &RemoteDelta::InventoryReceived(true),
            43,
        ))
        .expect("inventory received json");
        assert_eq!(message["t"], "inventory_received");
        assert_eq!(message["seq"], 43);
        assert_eq!(message["d"], true);
    }

    /// A watcher still wants to know WHERE a character is -- that drives the
    /// "different room" cue -- but not the prose describing it.
    #[test]
    fn watch_mode_keeps_room_identity_without_prose() {
        let mut state = RemoteStateSnapshot::default();
        state.room_name = Some("Town Square".to_string());
        state.room_id = Some("12345".to_string());
        state.exits = vec!["north".to_string()];

        let d = snap_json(SubscribeMode::Watch, &state);
        assert_eq!(d["room"]["name"], "Town Square");
        assert_eq!(d["room"]["id"], "12345");
        // `description` is skipped when empty, so it drops out of the JSON
        // entirely rather than shipping an empty array.
        assert!(
            d["room"].get("description").is_none(),
            "room prose must not ship to a watcher: {}",
            d["room"]
        );

        // Play mode still carries the prose for the same state.
        let mut with_prose = RemoteStateSnapshot::default();
        with_prose.room_name = Some("Town Square".to_string());
        with_prose.room_description = vec![crate::data::widget::StyledLine {
            segments: vec![TextSegment::plain("A wide plaza.")],
            stream: "main".to_string(),
            timestamp: None,
        }];
        let d = snap_json(SubscribeMode::Play, &with_prose);
        assert!(d["room"].get("description").is_some());
    }

    #[test]
    fn watch_mode_delta_filter_keeps_status_drops_bulk() {
        use crate::core::remote::RemoteDelta as D;

        let watch = SubscribeMode::Watch;
        let play = SubscribeMode::Play;

        let text = D::Text(RemoteLine {
            seq: 1,
            stream: "main".to_string(),
            line: Arc::new(crate::data::widget::StyledLine {
                segments: vec![TextSegment::plain("x")],
                stream: "main".to_string(),
                timestamp: None,
            }),
        });
        assert!(!watch.wants(&text), "text is the bulk of the feed");
        assert!(play.wants(&text), "the phone needs it -- it IS the game");

        let vitals = D::Vitals(Default::default());
        assert!(watch.wants(&vitals));
        assert!(play.wants(&vitals));

        let group = D::Group(Default::default());
        assert!(watch.wants(&group), "grouping is what the display shows");

        let indicators = D::Indicators(Default::default());
        assert!(watch.wants(&indicators));

        // Play mode is unfiltered by construction.
        let spells = D::Spells(Vec::new());
        assert!(!watch.wants(&spells));
        assert!(play.wants(&spells));

        let inventory_tree = D::InventoryTree(Some(inventory_tree()));
        assert!(!watch.wants(&inventory_tree));
        assert!(play.wants(&inventory_tree));

        let inventory_received = D::InventoryReceived(true);
        assert!(!watch.wants(&inventory_received));
        assert!(play.wants(&inventory_received));
    }

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
                custom_launch: None,
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
                custom_launch: None,
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
                    custom_launch: None,
                })
            );
        }
        // Lich attach with a launch command (mobile cold-start): the command
        // rides the connect and only parses in lich mode.
        assert_eq!(
            parse_client_message(
                r#"{"t":"connect","d":{"mode":"lich","host":"100.64.0.7","port":8001,"custom_launch":"rubyw lich.rbw --detachable-client=8001"}}"#
            ),
            Some(ClientMessage::Connect {
                profile: None,
                account: None,
                password: None,
                character: None,
                game: None,
                save_password: false,
                profile_name: None,
                lich_host: Some("100.64.0.7".to_string()),
                lich_port: Some(8001),
                custom_launch: Some("rubyw lich.rbw --detachable-client=8001".to_string()),
            })
        );
        // A launch command in DIRECT mode is ignored (lich-only).
        assert!(matches!(
            parse_client_message(
                r#"{"t":"connect","d":{"account":"ACCT","character":"Testy","custom_launch":"x"}}"#
            ),
            Some(ClientMessage::Connect {
                custom_launch: None,
                ..
            })
        ));
        // Lich mode without a complete target or profile → rejected.
        assert_eq!(
            parse_client_message(r#"{"t":"connect","d":{"mode":"lich","host":"pc.local"}}"#),
            None
        );
        // Lich mode by saved profile name alone is fine.
        assert!(matches!(
            parse_client_message(r#"{"t":"connect","d":{"mode":"lich","profile":"Home"}}"#),
            Some(ClientMessage::Connect {
                profile: Some(_),
                ..
            })
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
            webui_available: false,
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
    fn parse_wheel_pick_messages() {
        assert_eq!(
            parse_client_message(r#"{"t":"wheel_pick","d":{"key":"","path":[2]}}"#),
            Some(ClientMessage::WheelPick {
                key: String::new(),
                path: vec![2]
            })
        );
        // Named wheel, folder descent; a missing key means the default.
        assert_eq!(
            parse_client_message(r#"{"t":"wheel_pick","d":{"key":"spells","path":[1,0]}}"#),
            Some(ClientMessage::WheelPick {
                key: "spells".to_string(),
                path: vec![1, 0]
            })
        );
        assert_eq!(
            parse_client_message(r#"{"t":"wheel_pick","d":{"path":[0]}}"#),
            Some(ClientMessage::WheelPick {
                key: String::new(),
                path: vec![0]
            })
        );
        // Empty, missing or non-numeric paths → rejected.
        assert_eq!(
            parse_client_message(r#"{"t":"wheel_pick","d":{"key":"","path":[]}}"#),
            None
        );
        assert_eq!(parse_client_message(r#"{"t":"wheel_pick","d":{}}"#), None);
        assert_eq!(
            parse_client_message(r#"{"t":"wheel_pick","d":{"path":["x"]}}"#),
            None
        );
    }

    #[test]
    fn wheels_delta_ships_structure_without_commands() {
        use crate::core::remote::RemoteWheelSlice;
        let w = RemoteWheels {
            default: vec![
                RemoteWheelSlice {
                    label: "look".to_string(),
                    client: None,
                    color: None,
                    span: None,
                    inner: Some(65),
                    back: false,
                    slices: vec![],
                },
                RemoteWheelSlice {
                    label: "stance".to_string(),
                    client: None,
                    color: Some("#2e8b57".to_string()),
                    span: Some(120.0),
                    inner: None,
                    back: false,
                    slices: vec![RemoteWheelSlice {
                        label: "defensive".to_string(),
                        client: None,
                        color: None,
                        span: None,
                        inner: None,
                        back: false,
                        slices: vec![],
                    }],
                },
            ],
            named: Default::default(),
            tuning: crate::core::remote::RemoteWheelTuning {
                movement_stick: "left".to_string(),
                back_slice: "down".to_string(),
                deadzone: 50,
                aim_dwell_ms: 150,
                nav_dwell_ms: 150,
                fire_debounce_ms: 300,
                release_grace_ms: 40,
                fire_mode: "retract".to_string(),
                edge_threshold: 90,
                retract_delta: 10,
            },
            wheel_stick: std::iter::once(("exits".to_string(), "right".to_string())).collect(),
            wheel_start: std::iter::once(("combat".to_string(), -30.0_f32)).collect(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&delta(&RemoteDelta::Wheels(Arc::new(w)), 5)).unwrap();
        assert_eq!(json["t"], "wheels");
        assert_eq!(json["d"]["default"][0]["label"], "look");
        assert_eq!(json["d"]["default"][1]["color"], "#2e8b57");
        assert_eq!(json["d"]["default"][1]["slices"][0]["label"], "defensive");
        // Commands are resolved server-side on pick; they never ship.
        assert!(json["d"]["default"][0].get("command").is_none());
        // Tuning + per-wheel stick ride along so the phone matches feel.
        assert_eq!(json["d"]["tuning"]["aim_dwell_ms"], 150);
        assert_eq!(json["d"]["tuning"]["back_slice"], "down");
        // Fire mode + thresholds ship so the phone controller honors them.
        assert_eq!(json["d"]["tuning"]["fire_mode"], "retract");
        assert_eq!(json["d"]["tuning"]["edge_threshold"], 90);
        assert_eq!(json["d"]["tuning"]["retract_delta"], 10);
        assert_eq!(json["d"]["wheel_stick"]["exits"], "right");
        // Variable-width fields ship: explicit spans/inners per slice
        // (absent = even share / global deadzone) and per-wheel start.
        assert_eq!(json["d"]["default"][1]["span"], 120.0);
        assert_eq!(json["d"]["default"][0]["inner"], 65);
        assert!(json["d"]["default"][0].get("span").is_none());
        assert!(json["d"]["default"][1].get("inner").is_none());
        assert_eq!(json["d"]["wheel_start"]["combat"], -30.0);
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
    fn parse_settings_messages() {
        assert_eq!(
            parse_client_message(r#"{"t":"settings_get","d":{"request_id":11}}"#),
            Some(ClientMessage::SettingsGet { request_id: 11 })
        );
        assert_eq!(
            parse_client_message(
                r#"{"t":"settings_put","d":{"request_id":12,"key":"ui.buffer_size","value":5000,"scope":"character"}}"#
            ),
            Some(ClientMessage::SettingsPut {
                request_id: 12,
                key: "ui.buffer_size".to_string(),
                value: serde_json::json!(5000),
                scope: "character".to_string(),
                clear: false,
            })
        );
        // Optional clear flag (sensitive optional-text reset).
        assert_eq!(
            parse_client_message(
                r#"{"t":"settings_put","d":{"request_id":14,"key":"connection.password","value":"","scope":"character","clear":true}}"#
            ),
            Some(ClientMessage::SettingsPut {
                request_id: 14,
                key: "connection.password".to_string(),
                value: serde_json::json!(""),
                scope: "character".to_string(),
                clear: true,
            })
        );
        // Non-scalar values (lists) pass through as JSON for the handler.
        assert!(matches!(
            parse_client_message(
                r#"{"t":"settings_put","d":{"request_id":13,"key":"tts.gags","value":["a","b"],"scope":"global"}}"#
            ),
            Some(ClientMessage::SettingsPut { scope, .. }) if scope == "global"
        ));
        // Unknown scope or missing fields → rejected.
        assert_eq!(
            parse_client_message(
                r#"{"t":"settings_put","d":{"request_id":12,"key":"k","value":1,"scope":"profile"}}"#
            ),
            None
        );
        assert_eq!(
            parse_client_message(r#"{"t":"settings_put","d":{"request_id":12,"key":"k"}}"#),
            None
        );
    }

    #[test]
    fn parse_streams_messages() {
        assert_eq!(
            parse_client_message(r#"{"t":"streams_get","d":{"request_id":31}}"#),
            Some(ClientMessage::StreamsGet { request_id: 31 })
        );
        assert_eq!(
            parse_client_message(
                r#"{"t":"streams_put","d":{"request_id":32,"stream":"bounty","target":"window:bounty_win"}}"#
            ),
            Some(ClientMessage::StreamsPut {
                request_id: 32,
                stream: "bounty".to_string(),
                target: "window:bounty_win".to_string(),
            })
        );
        // Missing target → rejected.
        assert_eq!(
            parse_client_message(r#"{"t":"streams_put","d":{"request_id":32,"stream":"bounty"}}"#),
            None
        );
    }

    #[test]
    fn streams_delta_shapes() {
        // Get reply: catalog fields ride at the payload top level.
        let d = RemoteDelta::Streams {
            client_id: 4,
            request_id: 33,
            data: serde_json::json!({
                "streams": [{ "id": "bounty", "destination": "Main" }],
                "windows": ["main", "thoughts"],
                "fallback": "main",
            }),
            stream: None,
            error: None,
            saved: false,
        };
        let json: serde_json::Value = serde_json::from_str(&delta(&d, 2)).unwrap();
        assert_eq!(json["t"], "streams");
        assert_eq!(json["d"]["request_id"], 33);
        assert_eq!(json["d"]["streams"][0]["id"], "bounty");
        assert_eq!(json["d"]["windows"][1], "thoughts");
        assert_eq!(json["d"]["fallback"], "main");
        assert!(
            json["d"].get("client_id").is_none(),
            "client_id stays server-side"
        );

        // Put reply: no catalog, echoes the stream, carries saved/error.
        let d = RemoteDelta::Streams {
            client_id: 4,
            request_id: 34,
            data: serde_json::Value::Null,
            stream: Some("bounty".to_string()),
            error: None,
            saved: true,
        };
        let json: serde_json::Value = serde_json::from_str(&delta(&d, 2)).unwrap();
        assert_eq!(json["d"]["request_id"], 34);
        assert_eq!(json["d"]["stream"], "bounty");
        assert_eq!(json["d"]["saved"], true);
        assert!(json["d"].get("streams").is_none());
    }

    #[test]
    fn settings_delta_shape() {
        let d = RemoteDelta::Settings {
            client_id: 4,
            request_id: 21,
            catalog: serde_json::Value::Null,
            key: Some("ui.buffer_size".to_string()),
            error: None,
            saved: true,
        };
        let json: serde_json::Value = serde_json::from_str(&delta(&d, 3)).unwrap();
        assert_eq!(json["t"], "settings");
        assert_eq!(json["d"]["request_id"], 21);
        assert_eq!(json["d"]["key"], "ui.buffer_size");
        assert_eq!(json["d"]["saved"], true);
        assert!(json["d"]["error"].is_null());
        // client_id stays server-side.
        assert!(json["d"].get("client_id").is_none());
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
                custom_launch: None,
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
                custom_launch: Some("rubyw lich.rbw --detachable-client=8000".to_string()),
            },
        ];
        let json: serde_json::Value = serde_json::from_str(&profiles(&list, 9)).unwrap();
        assert_eq!(json["t"], "profiles");
        assert_eq!(json["d"]["list"][0]["account_masked"], "MY*******");
        assert_eq!(json["d"]["list"][0]["has_password"], true);
        assert_eq!(json["d"]["list"][0]["mode"], "direct");
        // Direct entries omit the Lich target fields entirely.
        assert!(json["d"]["list"][0].get("host").is_none());
        assert!(json["d"]["list"][0].get("custom_launch").is_none());
        assert_eq!(json["d"]["list"][1]["mode"], "lich");
        assert_eq!(json["d"]["list"][1]["host"], "100.64.0.7");
        assert_eq!(json["d"]["list"][1]["port"], 8000);
        // The launch command is exposed so the client can show/edit it.
        assert_eq!(
            json["d"]["list"][1]["custom_launch"],
            "rubyw lich.rbw --detachable-client=8000"
        );
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
