//! The transport half of the multi-account display: find the other VellumFE
//! instances on this machine and keep a status-only websocket open to each.
//!
//! Every running instance already registers itself in the machine-local
//! Vellum runtime registry and serves the same websocket the
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
    /// Set by every peer mutation; cleared when the render snapshot rebuilds.
    /// Status changes arrive a few times a second while frames render at 60,
    /// so most frames reuse the cached Arc instead of cloning the table.
    dirty: Arc<std::sync::atomic::AtomicBool>,
    cache: Mutex<Arc<BTreeMap<u16, PeerStatus>>>,
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
        let dirty = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);

        tokio::spawn(discovery_loop(
            peers.clone(),
            dirty.clone(),
            token,
            shutdown_rx,
        ));

        Self {
            peers,
            dirty,
            cache: Mutex::new(Arc::new(BTreeMap::new())),
            shutdown,
        }
    }

    /// Reap lost peers and hand back the render snapshot, in ONE lock
    /// acquisition. The old shape took the mutex twice per frame (reap, then
    /// a full deep clone) while every socket task contended for the same
    /// lock; now the clone happens only when something actually changed.
    pub fn reap_and_snapshot(&self, now_ms: u64) -> Arc<BTreeMap<u16, PeerStatus>> {
        use std::sync::atomic::Ordering;
        let mut peers = self.peers.lock().expect("peer table poisoned");
        let before = peers.len();
        peers.retain(|_, p| p.freshness(now_ms) != super::Freshness::Lost);
        if peers.len() != before {
            self.dirty.store(true, Ordering::Relaxed);
        }
        let mut cache = self.cache.lock().expect("hub cache poisoned");
        if self.dirty.swap(false, Ordering::Relaxed) {
            *cache = Arc::new(peers.clone());
        }
        cache.clone()
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
    dirty: Arc<std::sync::atomic::AtomicBool>,
    token: String,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let own_pid = std::process::id();
    // Ports with a live peer_task. SHARED with the tasks (not a local set):
    // a task that gives up after repeated failures removes its port, so a
    // sibling that restarts under the same or a new port gets a fresh task.
    // The old insert-only local set orphaned every restarted sibling.
    let watching: Arc<Mutex<std::collections::HashSet<u16>>> =
        Arc::new(Mutex::new(std::collections::HashSet::new()));
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
            if !watching
                .lock()
                .expect("watch set poisoned")
                .insert(entry.port)
            {
                continue;
            }
            tokio::spawn(peer_task(
                entry.port,
                entry.character.clone(),
                token.clone(),
                peers.clone(),
                dirty.clone(),
                watching.clone(),
                shutdown.clone(),
            ));
        }
    }
}

/// Give up on a peer after this many CONSECUTIVE failed connections (~2
/// minutes at the reconnect delay). The port is then released back to
/// discovery, which respawns a task only while the registry still advertises
/// a live pid there -- so a genuinely dead instance stops being dialed, and
/// a restarted one is picked up fresh (with its current character name).
const MAX_CONSECUTIVE_FAILURES: u32 = 24;

/// Keep one peer connected, reconnecting until shutdown or sustained failure.
async fn peer_task(
    port: u16,
    character: String,
    token: String,
    peers: PeerTable,
    dirty: Arc<std::sync::atomic::AtomicBool>,
    watching: Arc<Mutex<std::collections::HashSet<u16>>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    // One roster request per peer per hub lifetime, not per reconnect -- a
    // flapping sidecar must not turn into a stream of `group` commands in
    // that player's session.
    let mut sent_group = false;
    let mut consecutive_failures: u32 = 0;

    loop {
        if *shutdown.borrow() {
            return;
        }

        match run_peer(port, &character, &token, &peers, &dirty, &mut sent_group).await {
            Ok(()) => {
                consecutive_failures = 0;
                tracing::debug!("multiaccount: peer {character} on {port} closed");
            }
            Err(err) => {
                consecutive_failures += 1;
                tracing::debug!("multiaccount: peer {character} on {port}: {err}");
            }
        }

        // Mark disconnected but KEEP the last-known status: a card that
        // blanks on every brief reconnect is worse than one that dims.
        // Stamping the clock here matters -- freshness for a disconnected
        // peer measures time since the DROP. Without the stamp, a peer that
        // was quiet before dropping was reaped almost immediately.
        if let Ok(mut table) = peers.lock() {
            if let Some(peer) = table.get_mut(&port) {
                peer.connected = false;
                peer.last_update_ms = now_ms();
                dirty.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }

        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            tracing::info!(
                "multiaccount: giving up on {character} at {port} after                  {consecutive_failures} failed connections; discovery will                  retry if the registry still lists it"
            );
            watching.lock().expect("watch set poisoned").remove(&port);
            return;
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
    dirty: &std::sync::atomic::AtomicBool,
    sent_group: &mut bool,
) -> anyhow::Result<()> {
    let url = format!("ws://127.0.0.1:{port}/ws");
    let request = url.into_client_request()?;
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await?;

    // Auth must be the first frame; the shared pairing token is the same file
    // every instance on this machine reads, so no pairing step is needed.
    socket
        .send(Message::Text(
            serde_json::json!({"t": "auth", "d": {"token": token}})
                .to_string()
                .into(),
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
            serde_json::json!({"t": "resume", "d": {"seq": 0}})
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
        dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    while let Some(frame) = socket.next().await {
        match frame? {
            Message::Text(text) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    let is_snapshot = value.get("t").and_then(|v| v.as_str()) == Some("snapshot");
                    let needs_roster;
                    {
                        let mut table = peers.lock().expect("peer table poisoned");
                        // entry(), not get_mut(): if the reaper removed this
                        // port while the socket stayed open, later frames
                        // must re-materialize the card rather than be
                        // silently discarded forever.
                        let peer = table.entry(port).or_insert_with(|| PeerStatus {
                            character: character.to_string(),
                            port,
                            ..Default::default()
                        });
                        apply_frame(peer, &value);
                        dirty.store(true, std::sync::atomic::Ordering::Relaxed);
                        needs_roster =
                            is_snapshot && peer.group.is_grouped() && !peer.group.confirmed;
                    }
                    // Ask the peer to confirm its roster ONLY when the
                    // snapshot shows it grouped without one -- a session
                    // grouped before it started parsing has no message left
                    // to replay. Ungrouped or already-confirmed peers get no
                    // command at all, and `sent_group` persists across
                    // reconnects so a flapping sidecar cannot turn this into
                    // a command stream in that player's session.
                    if needs_roster && !*sent_group {
                        *sent_group = true;
                        socket
                            .send(Message::Text(
                                serde_json::json!({"t": "cmd", "d": {"text": "group"}})
                                    .to_string()
                                    .into(),
                            ))
                            .await?;
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
            // The peer's current name, authoritative over the registry label
            // captured at discovery -- that label can be a pre-login
            // "default" or a recycled port's previous character, and
            // clustering resolves rosters by name.
            if let Some(name) = d
                .get("character")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                peer.character = name.to_string();
            }

            apply_vitals(peer, d.get("vitals"));
            apply_indicators(peer, d.get("indicators"));
            apply_injuries(peer, d.get("injuries"));
            apply_rt(peer, d.get("rt"));
            apply_room(peer, d.get("room"));
            apply_effects(peer, d.get("effects"));
            apply_hands(peer, d.get("hands"));

            // A SNAPSHOT is authoritative: fields the encoder skips when
            // empty (group, minivitals, gauges) are ABSENT precisely when the
            // peer has none -- so absence here means "clear", not
            // "unchanged". Deltas below keep absent-as-unchanged, which is
            // correct for them. Without this split, a disband that happened
            // across a reconnect left the old roster displayed as a
            // confirmed group.
            match d.get("group") {
                Some(g) => apply_group(peer, Some(g)),
                None => {
                    peer.group = crate::core::group::GroupState {
                        confirmed: true,
                        ..Default::default()
                    }
                }
            }
            match d.get("minivitals") {
                Some(v) => apply_minivitals(peer, Some(v)),
                None => peer.minivitals.clear(),
            }
            if d.get("char_info").and_then(|c| c.get("gauges")).is_some() {
                apply_char_info(peer, d.get("char_info"));
            } else {
                peer.mind = None;
                peer.encumbrance = None;
                peer.stance = None;
                peer.field_exp = None;
            }
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
            peer.prepared_spell = d.get("spell").and_then(|v| v.as_str()).map(str::to_string);
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
    // Typed decode against the SAME struct the encoder serializes, so a
    // field rename is a parse difference here rather than a silent stale
    // value. Absent fields deserialize to None and leave the previous value
    // alone: char_info ships only on change, and a missing section means
    // "unchanged", not "now unknown".
    let Ok(gauges) = serde_json::from_value::<crate::core::remote::RemoteGauges>(gauges.clone())
    else {
        return;
    };
    let to_gauge = |g: crate::core::remote::RemoteGauge| Gauge {
        value: g.value,
        text: g.text,
    };
    if let Some(g) = gauges.mind {
        peer.mind = Some(to_gauge(g));
    }
    if let Some(g) = gauges.encumbrance {
        peer.encumbrance = Some(to_gauge(g));
    }
    if let Some(g) = gauges.stance {
        peer.stance = Some(to_gauge(g));
    }
    if let Some(fxp) = gauges.field_exp {
        peer.field_exp = Some((fxp.value, fxp.max));
    }
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
    let Some(v) = v else { return };
    let Ok(vitals) = serde_json::from_value::<Vec<crate::core::remote::RemoteVital>>(v.clone())
    else {
        return;
    };
    peer.minivitals = vitals
        .into_iter()
        .map(|vital| (vital.id, (vital.value, vital.max)))
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
    peer.room_name = v.get("name").and_then(|x| x.as_str()).map(str::to_string);
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
        assert_eq!(
            p.stance.as_ref().map(|g| g.text.as_str()),
            Some("defensive")
        );
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
        assert!(
            !p.group.confirmed,
            "unconfirmed must survive the round trip"
        );
    }

    /// A snapshot is authoritative: the encoder skips `group` when the peer
    /// is ungrouped, so its ABSENCE clears -- otherwise a disband that
    /// happened across a reconnect left the old roster shown as confirmed.
    #[test]
    fn a_snapshot_without_group_clears_a_stale_roster() {
        let mut p = peer();
        p.group.replace(
            crate::core::group::GroupLeader::SelfLed,
            vec![crate::core::group::GroupMember {
                id: "-1".to_string(),
                noun: "bob".to_string(),
                name: "Bob".to_string(),
            }],
        );
        p.mind = Some(Gauge {
            value: 42,
            text: "muddled".to_string(),
        });
        p.minivitals.insert("health".to_string(), (51, 51));

        apply_frame(&mut p, &serde_json::json!({"t": "snapshot", "d": {}}));

        assert!(!p.group.is_grouped(), "stale roster must not survive");
        assert!(p.group.confirmed, "ungrouped is a known state, not a doubt");
        assert!(p.mind.is_none(), "absent gauges in a snapshot mean none");
        assert!(p.minivitals.is_empty());
    }

    /// Deltas keep absent-as-unchanged -- only snapshots clear.
    #[test]
    fn deltas_still_treat_absence_as_unchanged() {
        let mut p = peer();
        p.mind = Some(Gauge {
            value: 42,
            text: "muddled".to_string(),
        });
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "char_info", "d": {"gauges": {"stance": {"value": 50, "text": "forward"}}}}),
        );
        assert_eq!(p.mind.as_ref().map(|g| g.value), Some(42), "mind persists");
    }

    /// The wire name wins over the registry label captured at discovery --
    /// that label can be a pre-login "default" or a recycled port's previous
    /// character, and clustering resolves rosters by name.
    #[test]
    fn a_snapshot_updates_the_character_name() {
        let mut p = peer(); // registry said "Alice"
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "snapshot", "d": {"character": "Ultz"}}),
        );
        assert_eq!(p.character, "Ultz");

        // Absent or empty name keeps the label rather than blanking it.
        apply_frame(&mut p, &serde_json::json!({"t": "snapshot", "d": {}}));
        assert_eq!(p.character, "Ultz");
    }

    #[test]
    fn unknown_frames_are_ignored_without_touching_state() {
        let mut p = peer();
        p.last_update_ms = 5;
        // A newer peer may send frames this build does not model; that must
        // not count as an update or corrupt anything.
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "some_future_thing", "d": {}}),
        );
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
        assert_eq!(
            p.mind.as_ref().map(|g| g.value),
            Some(10),
            "mind must persist"
        );
        assert_eq!(p.stance.as_ref().map(|g| g.value), Some(50));
    }

    #[test]
    fn malformed_payloads_do_not_panic_or_clobber() {
        let mut p = peer();
        p.vitals.health = 77;
        // Wrong types everywhere; every applier must decline rather than
        // unwrap. A peer on a different build should never crash the display.
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "vitals", "d": "not an object"}),
        );
        apply_frame(&mut p, &serde_json::json!({"t": "injuries", "d": 42}));
        apply_frame(&mut p, &serde_json::json!({"t": "group", "d": []}));
        apply_frame(
            &mut p,
            &serde_json::json!({"t": "rt", "d": {"roundtime_end": "soon"}}),
        );
        assert_eq!(p.vitals.health, 77);
    }
}
