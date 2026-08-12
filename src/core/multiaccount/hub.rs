//! The transport half of the multi-account display: find the other VellumFE
//! instances on this machine and keep a status-only websocket open to each.
//!
//! Every running instance already registers itself in
//! `~/.vellum-fe/web-sessions/<pid>.json` and serves the same websocket the
//! phone uses, so nothing new is published here -- the hub is purely a
//! consumer. It connects with `subscribe {mode:"watch"}` so each peer sends
//! status and nothing else; without that a six-character setup would pull six
//! full text feeds (300 scrollback lines per stream, per peer) to render some
//! health bars.
//!
//! Our OWN instance is deliberately not dialed. It is in the registry like
//! any other, but its state is already in hand locally, and looping back
//! through a socket to read it would be silly.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

use super::{Gauge, PeerStatus};

/// How often to re-read the session registry looking for instances that
/// appeared or vanished. Cheap (a directory listing), and new characters
/// should show up promptly without being instant.
const DISCOVERY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Backoff between reconnect attempts to a peer that is refusing. Its process
/// may be starting up or shutting down; either way, hammering it helps
/// nobody.
const RECONNECT_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

/// Shared peer table, keyed by the peer's sidecar port.
pub type PeerTable = Arc<Mutex<BTreeMap<u16, PeerStatus>>>;

/// Handle to a running hub. Dropping it stops discovery and every peer task.
pub struct MultiAccountHub {
    peers: PeerTable,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl MultiAccountHub {
    /// Start discovering and connecting to sibling instances.
    ///
    /// Self-exclusion keys on PID, not port. The port is not known until our
    /// own sidecar finishes binding, and discovery can tick first -- when it
    /// does, nothing matches "self" and the instance opens a websocket to
    /// itself, showing a duplicate card. The pid is known immediately, never
    /// changes, and the registry already records it.
    pub fn start(token: String) -> Self {
        let peers: PeerTable = Arc::new(Mutex::new(BTreeMap::new()));
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);

        tokio::spawn(discovery_loop(peers.clone(), token, shutdown_rx));

        Self { peers, shutdown }
    }

    /// Snapshot of every known peer. Cheap clone; the display reads this once
    /// per frame rather than holding the lock while rendering.
    pub fn peers(&self) -> BTreeMap<u16, PeerStatus> {
        self.peers.lock().expect("peer table poisoned").clone()
    }

    /// Drop peers that have been gone long enough to not be coming back.
    /// Called by the display; kept separate from freshness so a peer dims for
    /// a while before it disappears.
    pub fn reap(&self, now_ms: u64) {
        let mut peers = self.peers.lock().expect("peer table poisoned");
        peers.retain(|_, p| p.freshness(now_ms) != super::Freshness::Lost);
    }
}

impl Drop for MultiAccountHub {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

/// Re-read the registry on an interval, starting a task for each new peer.
async fn discovery_loop(
    peers: PeerTable,
    token: String,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let own_pid = std::process::id();
    let mut connected: std::collections::HashSet<u16> = std::collections::HashSet::new();
    let mut ticker = tokio::time::interval(DISCOVERY_INTERVAL);

    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = shutdown.changed() => return,
        }

        // list_and_gc also reaps entries whose pid is dead, so a crashed
        // instance stops being advertised without our help.
        let entries = crate::core::session_registry::list_and_gc();
        for entry in entries {
            // Never dial ourselves: our own status is read locally, and a
            // loopback socket would render a second card for this character.
            if entry.pid == own_pid {
                continue;
            }
            if !connected.insert(entry.port) {
                continue;
            }
            tokio::spawn(peer_task(
                entry.port,
                entry.character.clone(),
                token.clone(),
                peers.clone(),
                shutdown.clone(),
            ));
        }
    }
}

/// Keep one peer connected, reconnecting until shutdown.
async fn peer_task(
    port: u16,
    character: String,
    token: String,
    peers: PeerTable,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        if *shutdown.borrow() {
            return;
        }

        match run_peer(port, &character, &token, &peers).await {
            Ok(()) => tracing::debug!("multiaccount: peer {character} on {port} closed"),
            Err(err) => tracing::debug!("multiaccount: peer {character} on {port}: {err}"),
        }

        // Mark disconnected but KEEP the last-known status: a card that
        // blanks on every brief reconnect is worse than one that dims.
        if let Ok(mut table) = peers.lock() {
            if let Some(peer) = table.get_mut(&port) {
                peer.connected = false;
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(RECONNECT_DELAY) => {}
            _ = shutdown.changed() => return,
        }
    }
}

/// One connection's lifetime: auth, subscribe as a watcher, then apply
/// frames until the socket closes.
async fn run_peer(
    port: u16,
    character: &str,
    token: &str,
    peers: &PeerTable,
) -> anyhow::Result<()> {
    let url = format!("ws://127.0.0.1:{port}/ws");
    let request = url.into_client_request()?;
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await?;

    // Auth must be the first frame; the shared pairing token is the same file
    // every instance on this machine reads, so no pairing step is needed.
    socket
        .send(Message::Text(
            serde_json::json!({"t": "auth", "d": {"token": token}}).to_string().into(),
        ))
        .await?;

    // Declare intent BEFORE resume, so the snapshot comes back already
    // trimmed to status rather than carrying a text feed we discard.
    socket
        .send(Message::Text(
            serde_json::json!({"t": "subscribe", "d": {"mode": "watch"}})
                .to_string()
                .into(),
        ))
        .await?;
    socket
        .send(Message::Text(
            serde_json::json!({"t": "resume", "d": {"seq": 0}}).to_string().into(),
        ))
        .await?;

    // Ask the peer to confirm its roster once, on connect. Group membership
    // is reconstructed from prose, and a session that was already grouped
    // before we connected has no message left to replay -- so without this
    // its roster stays unconfirmed forever. Exactly one `group` per peer per
    // connection: enough to be correct, few enough not to be command spam.
    socket
        .send(Message::Text(
            serde_json::json!({"t": "cmd", "d": {"text": "group"}})
                .to_string()
                .into(),
        ))
        .await?;

    {
        let mut table = peers.lock().expect("peer table poisoned");
        let peer = table.entry(port).or_insert_with(|| PeerStatus {
            character: character.to_string(),
            port,
            ..Default::default()
        });
        peer.character = character.to_string();
        peer.connected = true;
        peer.last_update_ms = now_ms();
    }

    while let Some(frame) = socket.next().await {
        match frame? {
            Message::Text(text) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    let mut table = peers.lock().expect("peer table poisoned");
                    if let Some(peer) = table.get_mut(&port) {
                        apply_frame(peer, &value);
                    }
                }
            }
            Message::Close(_) => break,
            // Ping/pong are handled by the library; other frames are not
            // part of this protocol.
            _ => {}
        }
    }

    Ok(())
}

/// Apply one server frame to a peer. Snapshot and delta share field shapes,
/// so both route through the same per-field appliers.
///
/// Unknown message types are ignored rather than treated as errors: a peer
/// may be a newer build that sends frames this one does not model.
fn apply_frame(peer: &mut PeerStatus, frame: &serde_json::Value) {
    let Some(kind) = frame.get("t").and_then(|v| v.as_str()) else {
        return;
    };
    let d = frame.get("d").unwrap_or(&serde_json::Value::Null);

    match kind {
        "snapshot" => {
            apply_vitals(peer, d.get("vitals"));
            apply_indicators(peer, d.get("indicators"));
            apply_injuries(peer, d.get("injuries"));
            apply_group(peer, d.get("group"));
            apply_rt(peer, d.get("rt"));
            apply_char_info(peer, d.get("char_info"));
            apply_room(peer, d.get("room"));
            apply_effects(peer, d.get("effects"));
            apply_hands(peer, d.get("hands"));
            apply_minivitals(peer, d.get("minivitals"));
            peer.prepared_spell = d
                .get("prepared_spell")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
        "vitals" => apply_vitals(peer, Some(d)),
        "indicators" => apply_indicators(peer, Some(d)),
        "injuries" => apply_injuries(peer, Some(d)),
        "group" => apply_group(peer, Some(d)),
        "rt" => apply_rt(peer, Some(d)),
        "char_info" => apply_char_info(peer, Some(d)),
        "room" => apply_room(peer, Some(d)),
        "effects" => apply_effects(peer, Some(d)),
        "hands" => apply_hands(peer, Some(d)),
        "minivitals" => apply_minivitals(peer, Some(d)),
        "prepared_spell" => {
            peer.prepared_spell = d
                .get("spell")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
        _ => return,
    }

    peer.last_update_ms = now_ms();
    peer.connected = true;
}

fn apply_vitals(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    if let Ok(vitals) = serde_json::from_value(v.clone()) {
        peer.vitals = vitals;
    }
}

fn apply_indicators(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    if let Ok(status) = serde_json::from_value(v.clone()) {
        peer.indicators = status;
    }
}

fn apply_injuries(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    if let Ok(injuries) = serde_json::from_value(v.clone()) {
        peer.injuries = injuries;
    }
}

fn apply_group(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    if let Ok(group) = serde_json::from_value(v.clone()) {
        peer.group = group;
    }
}

fn apply_rt(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    peer.roundtime_end = v.get("roundtime_end").and_then(|x| x.as_i64());
    peer.casttime_end = v.get("casttime_end").and_then(|x| x.as_i64());
    if let Some(t) = v.get("server_time").and_then(|x| x.as_i64()) {
        peer.server_time = t;
    }
}

fn apply_char_info(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(gauges) = v.and_then(|v| v.get("gauges")) else {
        return;
    };
    // Absent gauges leave the previous value alone rather than clearing it:
    // char_info ships only when something in it changed, and a missing
    // section means "unchanged", not "now unknown".
    if let Some(g) = read_gauge(gauges.get("mind")) {
        peer.mind = Some(g);
    }
    if let Some(g) = read_gauge(gauges.get("encumbrance")) {
        peer.encumbrance = Some(g);
    }
    if let Some(g) = read_gauge(gauges.get("stance")) {
        peer.stance = Some(g);
    }
}

fn read_gauge(v: Option<&serde_json::Value>) -> Option<Gauge> {
    let v = v?;
    Some(Gauge {
        value: v.get("value")?.as_u64()? as u32,
        text: v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

/// Effects ship as a flat array of category blocks; the card indexes them by
/// category, so they are re-keyed here rather than at every read.
fn apply_effects(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    let Ok(list) = serde_json::from_value::<Vec<crate::data::ActiveEffectsContent>>(v.clone())
    else {
        return;
    };
    peer.effects = list
        .into_iter()
        .map(|content| (content.category.clone(), content))
        .collect();
}

fn apply_minivitals(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(arr) = v.and_then(|v| v.as_array()) else {
        return;
    };
    peer.minivitals = arr
        .iter()
        .filter_map(|entry| {
            Some((
                entry.get("id")?.as_str()?.to_string(),
                (
                    entry.get("value")?.as_u64()? as u32,
                    entry.get("max")?.as_u64()? as u32,
                ),
            ))
        })
        .collect();
}

fn apply_hands(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    // Absent means empty-handed, not unchanged: the delta carries both slots
    // every time, so a missing key is a genuine "nothing there".
    peer.left_hand = v.get("left").and_then(|x| x.as_str()).map(str::to_string);
    peer.right_hand = v.get("right").and_then(|x| x.as_str()).map(str::to_string);
}

fn apply_room(peer: &mut PeerStatus, v: Option<&serde_json::Value>) {
    let Some(v) = v else { return };
    peer.room_name = v
        .get("name")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    peer.room_id = v.get("id").and_then(|x| x.as_str()).map(str::to_string);
}

/// Apply a raw server frame to a peer.
///
/// Exposed so the end-to-end wire suite can drive the appliers with frames
/// from a REAL server rather than hand-written JSON -- the unit tests cover
/// what the hub does with a frame, this covers that the frames it receives
/// actually have that shape.
pub fn apply_frame_for_test(peer: &mut PeerStatus, frame: &serde_json::Value) {
    apply_frame(peer, frame);
}

/// Local monotonic-ish milliseconds. Only ever compared against itself for
/// staleness, so wall-clock jumps would at worst dim a card for one tick.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer() -> PeerStatus {
        PeerStatus {
            character: "Alice".to_string(),
            port: 8040,
            ..Default::default()
        }
    }

    #[test]
    fn snapshot_frame_populates_every_status_field() {
        let mut p = peer();
        let frame = serde_json::json!({
            "t": "snapshot",
            "d": {
                "vitals": {"health": 80, "mana": 60, "stamina": 90, "spirit": 100},
                "indicators": {"stunned": true, "bleeding": false},
                "injuries": {"head": 2},
                "group": {
                    "leader": {"kind": "self_led"},
                    "members": [{"id": "-1", "noun": "bob", "name": "Bob"}],
                    "confirmed": true,
                    "generation": 3
                },
                "rt": {"roundtime_end": 1_700, "casttime_end": null, "server_time": 1_695},
                "char_info": {"gauges": {
                    "mind": {"value": 42, "text": "muddled"},
                    "stance": {"value": 80, "text": "defensive"}
                }},
                "room": {"name": "Town Square", "id": "12345"}
            }
        });
        apply_frame(&mut p, &frame);

        assert_eq!(p.vitals.health, 80);
        assert!(p.indicators.stunned());
        assert!(!p.indicators.bleeding());
        assert_eq!(p.injuries.get("head"), Some(&2));
        assert!(p.group.leads());
        assert_eq!(p.group.members[0].name, "Bob");
        assert_eq!(p.roundtime_end, Some(1_700));
        assert_eq!(p.roundtime_remaining(1_695), 5.0);
        assert_eq!(p.mind.as_ref().map(|g| g.value), Some(42));
        assert_eq!(p.stance.as_ref().map(|g| g.text.as_str()), Some("defensive"));
        // Encumbrance was not reported, so it stays unknown rather than 0.
        assert!(p.encumbrance.is_none());
        assert_eq!(p.room_name.as_deref(), Some("Town Square"));
        assert!(p.connected);
    }

    #[test]
    fn delta_frames_update_single_fields() {
        let mut p = peer();
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "vitals", "d": {"health": 25, "mana": 1, "stamina": 2, "spirit": 3}}),
        );
        assert_eq!(p.vitals.health, 25);

        apply_frame(
            &mut p,
            &serde_json::json!({"t": "indicators", "d": {"dead": true}}),
        );
        assert!(p.indicators.dead());
        // The earlier vitals must survive an unrelated delta.
        assert_eq!(p.vitals.health, 25);
    }

    #[test]
    fn a_group_delta_replaces_the_roster() {
        let mut p = peer();
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "group", "d": {
                "leader": {"kind": "other", "who": {"id": "-1", "noun": "bob", "name": "Bob"}},
                "members": [],
                "confirmed": false,
                "generation": 1
            }}),
        );
        assert!(!p.group.leads());
        assert!(!p.group.confirmed, "unconfirmed must survive the round trip");
    }

    #[test]
    fn unknown_frames_are_ignored_without_touching_state() {
        let mut p = peer();
        p.last_update_ms = 5;
        // A newer peer may send frames this build does not model; that must
        // not count as an update or corrupt anything.
        apply_frame(&mut p, &serde_json::json!({"t": "some_future_thing", "d": {}}));
        assert_eq!(p.last_update_ms, 5);
        assert!(!p.connected);

        apply_frame(&mut p, &serde_json::json!({"not_a_frame": true}));
        assert_eq!(p.last_update_ms, 5);
    }

    #[test]
    fn a_char_info_frame_without_a_gauge_leaves_the_old_value() {
        let mut p = peer();
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "char_info", "d": {"gauges": {"mind": {"value": 10, "text": "clear"}}}}),
        );
        assert_eq!(p.mind.as_ref().map(|g| g.value), Some(10));

        // char_info ships only on change; a frame omitting mind means
        // "unchanged", not "now unknown".
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "char_info", "d": {"gauges": {"stance": {"value": 50, "text": "forward"}}}}),
        );
        assert_eq!(p.mind.as_ref().map(|g| g.value), Some(10), "mind must persist");
        assert_eq!(p.stance.as_ref().map(|g| g.value), Some(50));
    }

    #[test]
    fn malformed_payloads_do_not_panic_or_clobber() {
        let mut p = peer();
        p.vitals.health = 77;
        // Wrong types everywhere; every applier must decline rather than
        // unwrap. A peer on a different build should never crash the display.
        apply_frame(&mut p, &serde_json::json!({"t": "vitals", "d": "not an object"}));
        apply_frame(&mut p, &serde_json::json!({"t": "injuries", "d": 42}));
        apply_frame(&mut p, &serde_json::json!({"t": "group", "d": []}));
        apply_frame(&mut p, &serde_json::json!({"t": "rt", "d": {"roundtime_end": "soon"}}));
        assert_eq!(p.vitals.health, 77);
    }
}
