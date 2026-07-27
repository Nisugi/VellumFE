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

use crate::config::{Config, MacrosConfig};
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
/// (`MacrosConfig::resolve`). Exception: type-in (`insert`) buttons ship
/// their text, since its whole purpose is to appear in the client's
/// input box; the client handles those taps locally.
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
    /// Type-in button: the client inserts `command` into its input box
    /// instead of sending the id back (a trailing `\r` also submits).
    pub insert: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<RemoteMacroOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// Phone-authored (macros-local.toml): may be edited/deleted remotely.
    pub editable: bool,
    /// The command behind an editable action button, echoed back so the
    /// phone editor can prefill its form. Hand-file commands stay private
    /// unless the button is type-in (`insert`) — that text is the client's
    /// to display by definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMacroOption {
    pub id: String,
    pub label: String,
    pub confirm: bool,
    /// Type-in option (see `RemoteMacroButton::insert`).
    pub insert: bool,
    /// Echoed for phone-authored buttons and type-in options, so the
    /// editor can prefill and insert taps stay client-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// Radial-wheel definitions serialized for remote clients: labels, wedge
/// tints and folder structure only — commands stay server-side and are
/// resolved by index path on activation (`Config::wheel_pick_command`),
/// so a stale client can never fire outdated command text. Colors are
/// palette-resolved to CSS hex before shipping.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteWheels {
    /// The effective default wheel (`[[controller_wheel]]`, falling back
    /// to `[controller_wheels.default]`), shown for a plain "wheel" bind.
    pub default: Vec<RemoteWheelSlice>,
    /// Named wheels, shown for "wheel:<name>" binds.
    pub named: std::collections::HashMap<String, Vec<RemoteWheelSlice>>,
    /// Input-feel tuning from `[controller_tuning]`, so the phone's dwell
    /// wheel matches the desktop feel (one source of truth, keybinds.toml).
    pub tuning: RemoteWheelTuning,
    /// Per-wheel aim-stick overrides by wheel name ("default" for the
    /// default wheel). Only the stick matters remotely — the button that
    /// opens a wheel is the client's own bind. None/absent = the phone's
    /// default (non-movement) stick.
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub wheel_stick: std::collections::HashMap<String, String>,
}

/// Wheel input-feel values mirrored to remote clients (see TuningConfig).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteWheelTuning {
    pub movement_stick: String,
    pub back_slice: String,
    pub deadzone: u8,
    pub aim_dwell_ms: u32,
    pub nav_dwell_ms: u32,
    pub fire_debounce_ms: u32,
    pub release_grace_ms: u32,
}

impl Default for RemoteWheelTuning {
    fn default() -> Self {
        let t = crate::config::TuningConfig::default();
        Self {
            movement_stick: t.movement_stick,
            back_slice: t.back_slice,
            deadzone: t.deadzone,
            aim_dwell_ms: t.aim_dwell_ms,
            nav_dwell_ms: t.nav_dwell_ms,
            fire_debounce_ms: t.fire_debounce_ms,
            release_grace_ms: t.release_grace_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteWheelSlice {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slices: Vec<RemoteWheelSlice>,
}

impl RemoteWheels {
    pub fn from_config(config: &Config) -> Self {
        fn wire_slices(config: &Config, slices: &[crate::config::WheelSlice]) -> Vec<RemoteWheelSlice> {
            slices
                .iter()
                .map(|slice| RemoteWheelSlice {
                    label: slice.label.clone(),
                    color: slice
                        .color
                        .as_deref()
                        .map(|c| config.resolve_palette_color(c)),
                    slices: wire_slices(config, &slice.slices),
                })
                .collect()
        }
        let t = &config.controller_tuning;
        Self {
            default: config
                .wheel_level_slices("", &[])
                .map(|slices| wire_slices(config, slices))
                .unwrap_or_default(),
            named: config
                .controller_wheels
                .iter()
                .map(|(name, slices)| (name.clone(), wire_slices(config, slices)))
                .collect(),
            tuning: RemoteWheelTuning {
                movement_stick: t.movement_stick.clone(),
                back_slice: t.back_slice.clone(),
                deadzone: t.deadzone,
                aim_dwell_ms: t.aim_dwell_ms,
                nav_dwell_ms: t.nav_dwell_ms,
                fire_debounce_ms: t.fire_debounce_ms,
                release_grace_ms: t.release_grace_ms,
            },
            wheel_stick: config
                .controller_wheels_meta
                .iter()
                .filter_map(|(name, meta)| {
                    meta.stick.clone().map(|s| (name.clone(), s))
                })
                .collect(),
        }
    }
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
                        insert: option.insert,
                        command: if button.editable || option.insert {
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
                insert: button.insert,
                x: button.x,
                y: button.y,
                editable: button.editable,
                command: if button.editable || button.insert {
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
        /// Room number when known (nav tag in direct mode, extracted from
        /// the room name under Lich).
        id: Option<String>,
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
    /// Radial-wheel definitions changed (keybinds reload or the desktop
    /// wheel editor saved); sent to every client.
    Wheels(Arc<RemoteWheels>),
    /// Active effects changed (spells/buffs/debuffs/cooldowns), in fixed
    /// category order.
    Effects(Vec<crate::data::ActiveEffectsContent>),
    /// Body-part injuries changed: id -> level (1-3 wounds, 4-6 scars);
    /// cleared parts are absent.
    Injuries(std::collections::HashMap<String, u8>),
    /// The targetable-creature list changed.
    Targets(Vec<RemoteTarget>),
    /// The room entity lists (interact mode) changed.
    Entities(RemoteRoomEntities),
    /// The room's portal list (dynamic portals wheel) changed.
    Portals(Vec<String>),
    /// Character-sheet lines changed (experience/encumbrance/bounty/society).
    CharInfo(RemoteCharInfo),
    /// Game-session status changed (headless runtime only).
    Session(RemoteSessionInfo),
    /// A highlight-triggered sound. Clients fetch the file from /sounds/
    /// and play it locally (the Android build has no native audio; the
    /// phone's browser engine is the sound device).
    Sound { file: String, volume: Option<f32> },
    /// The drawable map scene changed (location/sheet/building or a layout
    /// regeneration). Arc-shared with the snapshot watch.
    MapScene(Arc<RemoteMapScene>),
    /// Map position/ghost state changed (usually every room change).
    MapState(RemoteMapState),
    /// Reply to one client's map-locations request (the location picker).
    MapLocations {
        client_id: u64,
        request_id: u64,
        locations: Vec<String>,
    },
    /// Reply to one client's browse request: another location's scene (or
    /// why not). Sent when the layout is ready — generation is async.
    MapBrowse {
        client_id: u64,
        request_id: u64,
        location: String,
        scene: Option<Arc<RemoteMapScene>>,
        error: Option<String>,
    },
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
    /// Reply to one client's structured colors get/put (addressed).
    Colors {
        client_id: u64,
        request_id: u64,
        scope: String,
        colors: serde_json::Value,
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
    /// Reply to one client's registry settings get/put (addressed).
    /// `catalog` is the full setting list for gets (Null on put replies);
    /// `key` echoes the setting a put touched; `saved` marks a successful
    /// put.
    Settings {
        client_id: u64,
        request_id: u64,
        catalog: serde_json::Value,
        key: Option<String>,
        error: Option<String>,
        saved: bool,
    },
    /// Reply to one client's streams get/put (addressed). `data` is the
    /// catalog object (`{streams, windows, fallback}`) for gets, Null on
    /// put replies; `stream` echoes the stream a put touched.
    Streams {
        client_id: u64,
        request_id: u64,
        data: serde_json::Value,
        stream: Option<String>,
        error: Option<String>,
        saved: bool,
    },
}

/// Input from a remote client, drained by the active frontend's main loop
/// (TUI runtime loop / GUI pump) and fed through the same command path as
/// locally typed input.
#[derive(Clone, Debug)]
pub enum RemoteEvent {
    /// A command typed on a remote client.
    Command(String),
    /// The map location picker wants the list of mapped locations.
    MapLocations { client_id: u64, request_id: u64 },
    /// Browse another location's map (reply arrives once its layout is
    /// generated — that can be a moment after the request).
    MapView {
        client_id: u64,
        request_id: u64,
        location: String,
    },
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
    /// A radial-wheel slice picked on a remote client. `key` is "" for
    /// the default wheel or a named wheel; `path` indexes down to the
    /// leaf. The main loop resolves it against config
    /// (Config::wheel_pick_command) and runs the command through the
    /// same dispatch as typed input.
    WheelPick { key: String, path: Vec<usize> },
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
        /// Type-in button: the client inserts the text instead of
        /// sending the id (options carry their own flag).
        insert: bool,
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
        /// Set (both) for a Lich attach instead of a direct eAccess login.
        lich_host: Option<String>,
        lich_port: Option<u16>,
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
    /// The full settings catalog (registry dump + live values) for the
    /// phone settings sheet. Reply: `RemoteDelta::Settings` with `catalog`.
    SettingsGet { client_id: u64, request_id: u64 },
    /// Set one registered setting by dotted key. `value` is JSON typed by
    /// the setting's kind; `scope` is "character" or "global". Applied to
    /// the live config, persisted sparsely, then hot-refreshed where the
    /// server can. `clear` (sensitive optional-text keys only) resets the
    /// value to None — the only way a phone can unset a redacted secret,
    /// since redacted values never round-trip.
    SettingsPut {
        client_id: u64,
        request_id: u64,
        key: String,
        value: serde_json::Value,
        scope: String,
        clear: bool,
    },
    /// The streams catalog (every known stream + where it goes) for the
    /// phone Streams panel. Reply: `RemoteDelta::Streams` with `data`.
    StreamsGet { client_id: u64, request_id: u64 },
    /// Set one stream's orphan route. `target` is "discard", "main",
    /// "window:<name>", or "clear" (drop the route; fallback applies).
    /// Route editing only — window subscriptions stay desktop-edited.
    StreamsPut {
        client_id: u64,
        request_id: u64,
        stream: String,
        target: String,
    },
    /// Structured color config for the phone editor ("profile"/"global").
    ColorsGet {
        client_id: u64,
        request_id: u64,
        scope: String,
    },
    /// Validate and write the full color config, then hot-reload. The
    /// client edits the fetched JSON in place, so sections its UI doesn't
    /// cover survive the round trip.
    ColorsPut {
        client_id: u64,
        request_id: u64,
        scope: String,
        colors: serde_json::Value,
    },
}

/// Latest coalesced game state, published via `watch` so the server can
/// build a connect-time snapshot without asking the main loop.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteStateSnapshot {
    pub character: Option<String>,
    pub vitals: Vitals,
    pub room_name: Option<String>,
    /// Room number when known; overlaid by AppCore::flush_remote_state
    /// (nav/lich ids live on AppCore, not GameState).
    pub room_id: Option<String>,
    pub exits: Vec<String>,
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
    pub indicators: StatusInfo,
    pub roundtime_end: Option<i64>,
    pub casttime_end: Option<i64>,
    pub server_time: i64,
    /// Active effects in fixed category order (empty categories omitted).
    pub effects: Vec<crate::data::ActiveEffectsContent>,
    /// Body-part injuries: id -> level (1-3 wounds, 4-6 scars).
    pub injuries: std::collections::HashMap<String, u8>,
    /// Targetable creatures in the room (tap-to-target list).
    pub targets: Vec<RemoteTarget>,
    /// Room entity lists for interact mode (creatures/objects/players).
    pub entities: RemoteRoomEntities,
    /// The room's portal commands ("go arch"), for the dynamic portals
    /// wheel. Overlaid by AppCore::flush_remote_state (resolution needs
    /// the map service, which lives on AppCore).
    pub portals: Vec<String>,
    /// Character sheet: experience/encumbrance/bounty/society lines.
    pub char_info: RemoteCharInfo,
    /// Session status + session-control capability. Overlaid by the sink in
    /// `flush_state` (the sink owns it, not GameState).
    pub session: RemoteSessionInfo,
    /// Drawable map scene (overlaid by AppCore::flush_remote_state — the
    /// map lives on AppCore, not GameState). Pointer-compared.
    pub map_scene: RemoteMapSceneRef,
    /// Per-step map position/ghost state, paired with `map_scene`.
    pub map_state: RemoteMapState,
}

/// Room entity lists for the phone's interact mode — the same three
/// categories the desktop focus cycle reads from GameState (exits ride
/// the room payload). Labels are pre-built (creature statuses baked in)
/// and nouns pre-resolved with the last-word fallback, so activation is
/// a plain link-tap round-trip client-side.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteRoomEntities {
    pub creatures: Vec<RemoteRoomEntity>,
    pub objects: Vec<RemoteRoomEntity>,
    pub players: Vec<RemoteRoomEntity>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteRoomEntity {
    /// Exist id without the leading '#' (the link-tap exist_id).
    pub id: String,
    pub label: String,
    pub noun: String,
}

/// Menus want "hog", not "a muddy hog": last word of the display name
/// when the feed omitted a noun (mirror of interact.rs fallback_noun).
fn entity_noun(noun: Option<&str>, name: &str) -> String {
    noun.map(str::to_string)
        .unwrap_or_else(|| name.rsplit(' ').next().unwrap_or(name).to_string())
}

/// A targetable creature in the room, for the status drawer's tap-to-
/// target list. Tapping routes through the ordinary link-tap machinery.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteTarget {
    /// Exist id (e.g. "#146101714") — the link-tap exist_id.
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noun: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// True when this is the currently selected target.
    pub current: bool,
}

/// Character-sheet lines for the status drawer: experience, encumbrance,
/// bounty, society — pre-formatted core-side so every client renders the
/// same text. Empty sections are omitted from the wire.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteCharInfo {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub experience: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub encumbrance: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bounty: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub society: Vec<String>,
}

/// One drawable room on the phone map. Short field names on purpose — a
/// town's outdoor sheet is a few thousand of these per scene push.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMapRoom {
    /// mapdb room id (tap-to-travel target).
    pub i: u32,
    pub x: i32,
    pub y: i32,
    /// Entrance (door marker).
    #[serde(skip_serializing_if = "is_false")]
    pub e: bool,
}

/// One drawn edge. `k`: 0 = solid directional, 1 = dashed connector,
/// 2 = stub (draw short dashed arrows at both ends, labeled with the
/// partner room ids `ar`/`br`, instead of a long line).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMapEdge {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub k: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ar: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub br: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMapLabel {
    pub x: i32,
    pub y: i32,
    pub t: String,
}

/// The drawable slice of the map the phone shows — exactly what the desktop
/// mini map draws: one sheet, filtered to the current building when
/// indoors. Sent once per view change, not per step.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteMapScene {
    pub location: String,
    /// "outdoor" | "interiors"
    pub sheet: String,
    pub rooms: Vec<RemoteMapRoom>,
    pub edges: Vec<RemoteMapEdge>,
    pub labels: Vec<RemoteMapLabel>,
}

/// Scene handle with pointer-identity equality: scenes are large, and the
/// per-batch snapshot diff must not deep-compare thousands of rooms.
#[derive(Clone, Debug, Default)]
pub struct RemoteMapSceneRef(pub Option<Arc<RemoteMapScene>>);

impl PartialEq for RemoteMapSceneRef {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Serialize for RemoteMapSceneRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Ghost-room sketch node/edge (session-only unmapped interiors), placed
/// on the same cell grid as the scene.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteGhostNode {
    pub x: i32,
    pub y: i32,
    #[serde(skip_serializing_if = "is_false")]
    pub cur: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteGhostEdge {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l: Option<String>,
}

/// Active-trip progress shown on the phone map while `.go2` walks.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RemoteTravelStatus {
    /// Destination room id.
    pub dest: u32,
    pub done: usize,
    pub total: usize,
    /// Pre-formatted "1:04" ETA.
    pub eta: String,
}

/// Small per-step map state: where the character is on the current scene.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteMapState {
    /// Set while the walk executor is traveling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub travel: Option<RemoteTravelStatus>,
    /// Map data loaded and a room resolved — the client shows/hides its
    /// map button on this.
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Current mapdb room id (the highlight ring).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<u32>,
    /// Centering cell (the ghost's cell while in an unmapped room).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<[i32; 2]>,
    #[serde(skip_serializing_if = "is_false")]
    pub in_ghost: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ghosts: Vec<RemoteGhostNode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ghost_edges: Vec<RemoteGhostEdge>,
}

fn is_false(b: &bool) -> bool {
    !*b
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
            room_id: game_state.room_id.clone(),
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
            injuries: game_state.injuries.clone(),
            targets: {
                // dDBTarget narrows room creatures to the targetable set
                // (direct mode); when absent, every room creature counts.
                let ids = &game_state.target_list.target_ids;
                game_state
                    .room_creatures
                    .iter()
                    .filter(|c| ids.is_empty() || ids.contains(&c.id))
                    .map(|c| RemoteTarget {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        noun: c.noun.clone(),
                        status: c.status.clone(),
                        current: c.id == game_state.target_list.current_target,
                    })
                    .collect()
            },
            entities: RemoteRoomEntities {
                creatures: game_state
                    .room_creatures
                    .iter()
                    .map(|c| {
                        let statuses = c.display_statuses();
                        RemoteRoomEntity {
                            id: c.id.trim_start_matches('#').to_string(),
                            label: if statuses.is_empty() {
                                c.name.clone()
                            } else {
                                format!("{} ({})", c.name, statuses.join(", "))
                            },
                            noun: entity_noun(c.noun.as_deref(), &c.name),
                        }
                    })
                    .collect(),
                objects: game_state
                    .room_objects
                    .iter()
                    .map(|o| RemoteRoomEntity {
                        id: o.id.trim_start_matches('#').to_string(),
                        label: o.name.clone(),
                        noun: entity_noun(o.noun.as_deref(), &o.name),
                    })
                    .collect(),
                players: game_state
                    .room_players
                    .iter()
                    .map(|p| RemoteRoomEntity {
                        id: p.id.trim_start_matches('#').to_string(),
                        label: p.name.clone(),
                        noun: p.name.clone(),
                    })
                    .collect(),
            },
            char_info: {
                let mut info = RemoteCharInfo::default();
                let exp = &game_state.gs4_experience;
                if !exp.level_text.is_empty() {
                    info.experience.push(exp.level_text.clone());
                }
                if !exp.mind_state_text.is_empty() {
                    info.experience.push(format!(
                        "Mind: {} ({}%)",
                        exp.mind_state_text, exp.mind_state_value
                    ));
                }
                if !exp.next_level_text.is_empty() {
                    // nextLvlPB's value is raw experience, not a percent, and
                    // the text already carries it ("63667 until next level").
                    info.experience.push(format!("Next level: {}", exp.next_level_text));
                }
                let enc = &game_state.encumbrance;
                if !enc.text.is_empty() {
                    info.encumbrance.push(format!("{} ({}%)", enc.text, enc.value));
                    if !enc.blurb.is_empty() {
                        info.encumbrance.push(enc.blurb.clone());
                    }
                }
                if !game_state.bounty.compact_lines.is_empty() {
                    info.bounty = game_state.bounty.compact_lines.clone();
                } else if !game_state.bounty.raw_text.is_empty() {
                    info.bounty.push(game_state.bounty.raw_text.clone());
                }
                info.society = game_state.society.lines.clone();
                info
            },
            session: RemoteSessionInfo::default(),
            // Overlaid by AppCore::flush_remote_state (the map — and the
            // portal resolution that needs it — live there).
            portals: Vec::new(),
            map_scene: RemoteMapSceneRef::default(),
            map_state: RemoteMapState::default(),
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
    /// Latest radial-wheel definitions, for connect-time delivery.
    pub wheels_rx: watch::Receiver<Arc<RemoteWheels>>,
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
    wheels_tx: watch::Sender<Arc<RemoteWheels>>,
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
        let (wheels_tx, wheels_rx) = watch::channel(Arc::new(RemoteWheels::default()));
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
            wheels_rx,
            session,
            bound_port: bound_port.clone(),
        };
        (
            Self {
                buffer,
                delta_tx,
                state_tx,
                macros_tx,
                wheels_tx,
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

    /// Publish radial-wheel definitions: stored for connect-time delivery
    /// and broadcast to already-connected clients. Called on enable, on
    /// keybinds reload, and by the desktop wheel editor.
    pub fn set_wheels(&mut self, config: &Config) {
        let wheels = Arc::new(RemoteWheels::from_config(config));
        self.wheels_tx.send_replace(wheels.clone());
        let _ = self.delta_tx.send(RemoteDelta::Wheels(wheels));
    }

    /// Reply to one client's map-locations request.
    pub fn push_map_locations(&mut self, client_id: u64, request_id: u64, locations: Vec<String>) {
        let _ = self.delta_tx.send(RemoteDelta::MapLocations {
            client_id,
            request_id,
            locations,
        });
    }

    /// Reply to one client's map-browse request.
    pub fn push_map_browse(
        &mut self,
        client_id: u64,
        request_id: u64,
        location: String,
        scene: Option<Arc<RemoteMapScene>>,
        error: Option<String>,
    ) {
        let _ = self.delta_tx.send(RemoteDelta::MapBrowse {
            client_id,
            request_id,
            location,
            scene,
            error,
        });
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

    /// Route a structured colors reply to the requesting client.
    #[allow(clippy::too_many_arguments)]
    pub fn push_colors(
        &mut self,
        client_id: u64,
        request_id: u64,
        scope: String,
        colors: serde_json::Value,
        error: Option<String>,
        saved: bool,
    ) {
        let _ = self.delta_tx.send(RemoteDelta::Colors {
            client_id,
            request_id,
            scope,
            colors,
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

    /// Route a settings catalog / put reply to the requesting client.
    #[allow(clippy::too_many_arguments)]
    pub fn push_settings(
        &mut self,
        client_id: u64,
        request_id: u64,
        catalog: serde_json::Value,
        key: Option<String>,
        error: Option<String>,
        saved: bool,
    ) {
        let _ = self.delta_tx.send(RemoteDelta::Settings {
            client_id,
            request_id,
            catalog,
            key,
            error,
            saved,
        });
    }

    /// Route a streams catalog / put reply to the requesting client.
    #[allow(clippy::too_many_arguments)]
    pub fn push_streams(
        &mut self,
        client_id: u64,
        request_id: u64,
        data: serde_json::Value,
        stream: Option<String>,
        error: Option<String>,
        saved: bool,
    ) {
        let _ = self.delta_tx.send(RemoteDelta::Streams {
            client_id,
            request_id,
            data,
            stream,
            error,
            saved,
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
        if snap.room_name != self.last.room_name
            || snap.exits != self.last.exits
            || snap.room_id != self.last.room_id
        {
            let _ = self.delta_tx.send(RemoteDelta::Room {
                name: snap.room_name.clone(),
                exits: snap.exits.clone(),
                id: snap.room_id.clone(),
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
        if snap.injuries != self.last.injuries {
            let _ = self
                .delta_tx
                .send(RemoteDelta::Injuries(snap.injuries.clone()));
        }
        if snap.targets != self.last.targets {
            let _ = self
                .delta_tx
                .send(RemoteDelta::Targets(snap.targets.clone()));
        }
        if snap.entities != self.last.entities {
            let _ = self
                .delta_tx
                .send(RemoteDelta::Entities(snap.entities.clone()));
        }
        if snap.portals != self.last.portals {
            let _ = self
                .delta_tx
                .send(RemoteDelta::Portals(snap.portals.clone()));
        }
        if snap.char_info != self.last.char_info {
            let _ = self
                .delta_tx
                .send(RemoteDelta::CharInfo(snap.char_info.clone()));
        }
        // Scene before state, so a client never holds a position for a
        // scene it hasn't received.
        if snap.map_scene != self.last.map_scene {
            if let Some(scene) = &snap.map_scene.0 {
                let _ = self.delta_tx.send(RemoteDelta::MapScene(scene.clone()));
            }
        }
        if snap.map_state != self.last.map_state {
            let _ = self
                .delta_tx
                .send(RemoteDelta::MapState(snap.map_state.clone()));
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
            timestamp: None,
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
    fn flush_state_sends_map_deltas_by_pointer_identity() {
        let (mut sink, handles, _event_rx) = RemoteSink::new(100);
        let mut rx = handles.delta_tx.subscribe();

        let scene = Arc::new(RemoteMapScene {
            location: "Town".into(),
            sheet: "outdoor".into(),
            rooms: vec![RemoteMapRoom { i: 1, x: 0, y: 0, e: false }],
            edges: vec![],
            labels: vec![],
        });
        let mut snap = RemoteStateSnapshot::default();
        snap.map_scene = RemoteMapSceneRef(Some(scene.clone()));
        snap.map_state = RemoteMapState {
            available: true,
            room: Some(1),
            cell: Some([0, 0]),
            ..Default::default()
        };
        sink.flush_state(snap.clone());
        assert!(matches!(rx.try_recv(), Ok(RemoteDelta::MapScene(_))));
        assert!(matches!(rx.try_recv(), Ok(RemoteDelta::MapState(_))));
        assert!(rx.try_recv().is_err());

        // Same Arc + same state: nothing re-sent (the pointer compare must
        // not deep-compare thousands of rooms every batch).
        sink.flush_state(snap.clone());
        assert!(rx.try_recv().is_err());

        // Position moves, scene stays: only map_state goes out.
        snap.map_state.room = Some(2);
        snap.map_state.cell = Some([1, 0]);
        sink.flush_state(snap);
        assert!(matches!(rx.try_recv(), Ok(RemoteDelta::MapState(_))));
        assert!(rx.try_recv().is_err());
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
