//! Multi-account status: what every character on this machine is doing, in
//! one place.
//!
//! Each running VellumFE already publishes its own state over the web
//! sidecar's websocket and registers itself in `~/.vellum-fe/web-sessions/`.
//! So the pieces already exist: this module holds the *model* -- what a peer
//! is, how stale it is, and how peers cluster into groups -- while the
//! transport that fills it lives in `core::multiaccount::hub`.
//!
//! Deliberately pure and synchronous. Clustering from six independently
//! parsed rosters is the part with real logic in it, and it is much easier to
//! trust when it can be tested without sockets.

pub mod hub;

use std::collections::{BTreeMap, HashMap};

use crate::core::group::{GroupLeader, GroupState};
use crate::core::state::{StatusInfo, Vitals};

pub use hub::MultiAccountHub;

/// How long without an update before a peer is shown as stale rather than
/// current. Lich's groupbar uses 90s for the same purpose; a peer that has
/// gone quiet is usually mid-reconnect rather than gone.
pub const STALE_AFTER_MS: u64 = 90_000;

/// How long before a peer is dropped entirely. Past this it is not coming
/// back on its own -- the process is gone and the registry entry will have
/// been garbage-collected too.
pub const DROP_AFTER_MS: u64 = 120_000;

/// One numeric gauge, mirroring the wire's `{value, text}`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gauge {
    pub value: u32,
    pub text: String,
}

/// Everything the display knows about one character.
///
/// Fields that a session may not have reported are `Option`, so a card can
/// render "unknown" rather than a confident zero. A stance of 0 means fully
/// offensive; showing that for a character who never sent stance would be a
/// lie, not a default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PeerStatus {
    pub character: String,
    /// TCP port of that instance's web sidecar; also its identity here,
    /// since two instances cannot share one.
    pub port: u16,
    pub connected: bool,
    /// Local monotonic ms at the last update of any kind.
    pub last_update_ms: u64,

    pub vitals: Vitals,
    pub indicators: StatusInfo,
    pub injuries: HashMap<String, u8>,
    pub group: GroupState,

    pub mind: Option<Gauge>,
    pub encumbrance: Option<Gauge>,
    pub stance: Option<Gauge>,

    pub room_name: Option<String>,
    pub room_id: Option<String>,

    /// Absolute server timestamps, exactly as the feed gives them, so the
    /// display interpolates locally instead of receiving a tick per second.
    pub roundtime_end: Option<i64>,
    pub casttime_end: Option<i64>,
    pub server_time: i64,
}

/// How current a peer's data is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    /// Connected and reporting.
    Live,
    /// Connected but quiet for a while, or briefly disconnected. Render
    /// dimmed rather than removed -- a reconnect is the common case.
    Stale,
    /// Gone long enough that it is not coming back.
    Lost,
}

impl PeerStatus {
    pub fn freshness(&self, now_ms: u64) -> Freshness {
        let age = now_ms.saturating_sub(self.last_update_ms);
        if age >= DROP_AFTER_MS {
            Freshness::Lost
        } else if age >= STALE_AFTER_MS || !self.connected {
            Freshness::Stale
        } else {
            Freshness::Live
        }
    }

    /// Remaining roundtime in seconds, interpolated against the peer's own
    /// server clock. Absolute end timestamps are why this works without the
    /// peer streaming a countdown.
    pub fn roundtime_remaining(&self, now_server: i64) -> f32 {
        Self::remaining(self.roundtime_end, now_server)
    }

    pub fn casttime_remaining(&self, now_server: i64) -> f32 {
        Self::remaining(self.casttime_end, now_server)
    }

    fn remaining(end: Option<i64>, now_server: i64) -> f32 {
        end.map(|e| (e - now_server).max(0) as f32).unwrap_or(0.0)
    }
}

/// A set of characters the display should draw together.
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    /// Ports of the members, in display order: leader first when known.
    pub members: Vec<u16>,
    /// Port of the leader, when the leader is itself one of our characters.
    /// A group led by someone else's character has no port here -- we can see
    /// we follow them, but they are not on this machine.
    pub leader: Option<u16>,
    /// Display name of the leader even when they are not ours.
    pub leader_name: Option<String>,
    /// False when any member's roster is unconfirmed, so the display can say
    /// so instead of drawing a guess as fact.
    pub confirmed: bool,
}

impl Cluster {
    fn solo(port: u16) -> Self {
        Self {
            members: vec![port],
            leader: None,
            leader_name: None,
            confirmed: true,
        }
    }

    /// A single ungrouped character.
    pub fn is_solo(&self) -> bool {
        self.members.len() == 1 && self.leader_name.is_none()
    }
}

/// Group our characters into clusters using each one's own parsed roster.
///
/// The inputs disagree in practice: rosters are parsed independently on six
/// machines-worth of sessions, one may be mid-`group` reply, and a character
/// can be grouped with someone who is not ours at all. The rules:
///
/// - Two of our characters cluster when either one names the other, or when
///   both follow the same leader. Membership is by exist id, but our own
///   characters are matched by NAME, because a peer's roster names them as
///   the game does and we have no exist id for our own sessions.
/// - A group whose leader is not one of ours still forms a cluster, tagged
///   with the leader's name so the display can show who they follow.
/// - Unconfirmed rosters still cluster, but mark the cluster unconfirmed.
///
/// Clusters come back in a stable order (by lowest member port) so the
/// display does not reshuffle between frames.
pub fn cluster_peers(peers: &BTreeMap<u16, PeerStatus>) -> Vec<Cluster> {
    // Name -> port, for resolving one peer's roster entries to our own
    // characters. Names are compared case-insensitively; the game is
    // consistent but config and typing are not.
    let by_name: HashMap<String, u16> = peers
        .iter()
        .map(|(port, p)| (p.character.to_ascii_lowercase(), *port))
        .collect();

    // Union-find over ports, so a chain of pairwise links (A names B, B names
    // C) collapses into one cluster without needing a full roster from any
    // single peer.
    let mut parent: HashMap<u16, u16> = peers.keys().map(|p| (*p, *p)).collect();

    fn find(parent: &mut HashMap<u16, u16>, x: u16) -> u16 {
        let mut root = x;
        while parent[&root] != root {
            root = parent[&root];
        }
        // Path compression keeps repeated lookups cheap.
        let mut cur = x;
        while parent[&cur] != root {
            let next = parent[&cur];
            parent.insert(cur, root);
            cur = next;
        }
        root
    }

    fn union(parent: &mut HashMap<u16, u16>, a: u16, b: u16) {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            // Lower port wins, so cluster identity is stable across frames.
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            parent.insert(hi, lo);
        }
    }

    // Link peers that name each other.
    for (port, peer) in peers {
        if !peer.group.is_grouped() {
            continue;
        }
        for member in peer.group.everyone() {
            if let Some(other) = by_name.get(&member.name.to_ascii_lowercase()) {
                if other != port {
                    union(&mut parent, *port, *other);
                }
            }
        }
    }

    // Link peers that follow the same non-ours leader: two of our characters
    // both following someone else's leader are in one group even though
    // neither roster names the other (each may be unconfirmed).
    let mut by_foreign_leader: HashMap<String, Vec<u16>> = HashMap::new();
    for (port, peer) in peers {
        if let GroupLeader::Other(leader) = &peer.group.leader {
            let key = leader.name.to_ascii_lowercase();
            if !by_name.contains_key(&key) {
                by_foreign_leader.entry(key).or_default().push(*port);
            }
        }
    }
    for ports in by_foreign_leader.values() {
        for pair in ports.windows(2) {
            union(&mut parent, pair[0], pair[1]);
        }
    }

    // Collect the groups.
    let mut groups: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
    for port in peers.keys() {
        let root = find(&mut parent, *port);
        groups.entry(root).or_default().push(*port);
    }

    groups
        .into_values()
        .map(|mut members| {
            members.sort_unstable();
            if members.len() == 1 {
                let port = members[0];
                let peer = &peers[&port];
                // A lone character who follows someone not ours is still in a
                // group -- it just has one visible member.
                if let GroupLeader::Other(leader) = &peer.group.leader {
                    return Cluster {
                        members,
                        leader: None,
                        leader_name: Some(leader.name.clone()),
                        confirmed: peer.group.confirmed,
                    };
                }
                return Cluster::solo(port);
            }

            // Leader: one of ours who leads, else the shared foreign leader.
            let leader_port = members
                .iter()
                .copied()
                .find(|p| matches!(peers[p].group.leader, GroupLeader::SelfLed));
            let leader_name = leader_port
                .map(|p| peers[&p].character.clone())
                .or_else(|| {
                    members.iter().find_map(|p| match &peers[p].group.leader {
                        GroupLeader::Other(l) => Some(l.name.clone()),
                        _ => None,
                    })
                });

            // Leader first, then the rest by port, so the card order reads
            // the way the group does.
            if let Some(lp) = leader_port {
                members.retain(|p| *p != lp);
                members.insert(0, lp);
            }

            let confirmed = members.iter().all(|p| peers[p].group.confirmed);

            Cluster {
                members,
                leader: leader_port,
                leader_name,
                confirmed,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::group::GroupMember;

    fn member(name: &str) -> GroupMember {
        GroupMember {
            id: format!("-{}", name.len()),
            noun: name.to_ascii_lowercase(),
            name: name.to_string(),
        }
    }

    fn peer(port: u16, character: &str) -> PeerStatus {
        PeerStatus {
            character: character.to_string(),
            port,
            connected: true,
            last_update_ms: 1_000,
            ..Default::default()
        }
    }

    fn peers(list: Vec<PeerStatus>) -> BTreeMap<u16, PeerStatus> {
        list.into_iter().map(|p| (p.port, p)).collect()
    }

    #[test]
    fn ungrouped_characters_are_each_solo() {
        let map = peers(vec![peer(8040, "Alice"), peer(8041, "Bob")]);
        let clusters = cluster_peers(&map);
        assert_eq!(clusters.len(), 2);
        assert!(clusters.iter().all(|c| c.is_solo()));
    }

    #[test]
    fn a_leader_and_follower_form_one_cluster() {
        let mut alice = peer(8040, "Alice");
        alice
            .group
            .replace(GroupLeader::SelfLed, vec![member("Bob")]);
        let mut bob = peer(8041, "Bob");
        bob.group
            .replace(GroupLeader::Other(member("Alice")), vec![]);

        let clusters = cluster_peers(&peers(vec![alice, bob]));
        assert_eq!(clusters.len(), 1);
        let c = &clusters[0];
        assert_eq!(c.members, vec![8040, 8041], "leader sorts first");
        assert_eq!(c.leader, Some(8040));
        assert_eq!(c.leader_name.as_deref(), Some("Alice"));
        assert!(c.confirmed);
    }

    #[test]
    fn the_motivating_case_three_grouped_one_solo_two_grouped() {
        // "chars 1-3 are grouped, 4 is solo, and 5-6 are grouped as well"
        let mut a = peer(8040, "Alice");
        a.group.replace(
            GroupLeader::SelfLed,
            vec![member("Bob"), member("Carol")],
        );
        let mut b = peer(8041, "Bob");
        b.group
            .replace(GroupLeader::Other(member("Alice")), vec![member("Carol")]);
        let mut c = peer(8042, "Carol");
        c.group
            .replace(GroupLeader::Other(member("Alice")), vec![member("Bob")]);

        let d = peer(8043, "Dave"); // solo

        let mut e = peer(8044, "Eve");
        e.group.replace(GroupLeader::SelfLed, vec![member("Frank")]);
        let mut f = peer(8045, "Frank");
        f.group
            .replace(GroupLeader::Other(member("Eve")), vec![]);

        let clusters = cluster_peers(&peers(vec![a, b, c, d, e, f]));
        assert_eq!(clusters.len(), 3, "two groups and one solo: {clusters:?}");

        assert_eq!(clusters[0].members, vec![8040, 8041, 8042]);
        assert_eq!(clusters[0].leader_name.as_deref(), Some("Alice"));

        assert!(clusters[1].is_solo());
        assert_eq!(clusters[1].members, vec![8043]);

        assert_eq!(clusters[2].members, vec![8044, 8045]);
        assert_eq!(clusters[2].leader_name.as_deref(), Some("Eve"));
    }

    #[test]
    fn a_chain_of_partial_rosters_still_collapses_into_one_cluster() {
        // Only A names B and only B names C -- no single roster sees the whole
        // group. They must still cluster, which is why this is union-find and
        // not a per-peer grouping.
        let mut a = peer(8040, "Alice");
        a.group.replace(GroupLeader::SelfLed, vec![member("Bob")]);
        let mut b = peer(8041, "Bob");
        b.group
            .replace(GroupLeader::Other(member("Alice")), vec![member("Carol")]);
        let mut c = peer(8042, "Carol");
        c.group.mark_unconfirmed();

        let clusters = cluster_peers(&peers(vec![a, b, c]));
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members, vec![8040, 8041, 8042]);
    }

    #[test]
    fn two_of_ours_following_a_stranger_cluster_together() {
        // Neither roster names the other -- both only know they follow Zed,
        // who is not one of our characters.
        let mut a = peer(8040, "Alice");
        a.group.replace(GroupLeader::Other(member("Zed")), vec![]);
        a.group.mark_unconfirmed();
        let mut b = peer(8041, "Bob");
        b.group.replace(GroupLeader::Other(member("Zed")), vec![]);
        b.group.mark_unconfirmed();

        let clusters = cluster_peers(&peers(vec![a, b]));
        assert_eq!(clusters.len(), 1, "same foreign leader means same group");
        assert_eq!(clusters[0].leader_name.as_deref(), Some("Zed"));
        assert_eq!(clusters[0].leader, None, "Zed is not one of ours");
        assert!(
            !clusters[0].confirmed,
            "neither roster was confirmed, so the cluster is not either"
        );
    }

    #[test]
    fn a_lone_follower_of_a_stranger_is_not_solo() {
        let mut a = peer(8040, "Alice");
        a.group.replace(GroupLeader::Other(member("Zed")), vec![]);

        let clusters = cluster_peers(&peers(vec![a]));
        assert_eq!(clusters.len(), 1);
        assert!(!clusters[0].is_solo(), "she is grouped, just not with ours");
        assert_eq!(clusters[0].leader_name.as_deref(), Some("Zed"));
    }

    #[test]
    fn an_unconfirmed_member_marks_the_whole_cluster_unconfirmed() {
        let mut a = peer(8040, "Alice");
        a.group.replace(GroupLeader::SelfLed, vec![member("Bob")]);
        let mut b = peer(8041, "Bob");
        b.group
            .replace(GroupLeader::Other(member("Alice")), vec![]);
        b.group.mark_unconfirmed();

        let clusters = cluster_peers(&peers(vec![a, b]));
        assert_eq!(clusters.len(), 1);
        assert!(
            !clusters[0].confirmed,
            "the display must not present a partial roster as fact"
        );
    }

    #[test]
    fn names_match_case_insensitively() {
        let mut a = peer(8040, "Alice");
        a.group.replace(GroupLeader::SelfLed, vec![member("BOB")]);
        let b = peer(8041, "bob");

        let clusters = cluster_peers(&peers(vec![a, b]));
        assert_eq!(clusters.len(), 1, "casing must not split a group");
    }

    #[test]
    fn cluster_order_is_stable() {
        // The display must not reshuffle between frames.
        let mut a = peer(8045, "Alice");
        a.group.replace(GroupLeader::SelfLed, vec![member("Bob")]);
        let mut b = peer(8041, "Bob");
        b.group
            .replace(GroupLeader::Other(member("Alice")), vec![]);
        let d = peer(8043, "Dave");

        let map = peers(vec![a, b, d]);
        let first = cluster_peers(&map);
        let second = cluster_peers(&map);
        assert_eq!(first, second);
        // Clusters sort by their lowest member port: {8041,8045} then {8043}.
        assert_eq!(first[0].members, vec![8045, 8041], "leader first");
        assert_eq!(first[1].members, vec![8043]);
    }

    #[test]
    fn freshness_degrades_with_age_then_drops() {
        let p = peer(8040, "Alice"); // last_update_ms = 1_000
        assert_eq!(p.freshness(1_000), Freshness::Live);
        assert_eq!(p.freshness(1_000 + STALE_AFTER_MS - 1), Freshness::Live);
        assert_eq!(p.freshness(1_000 + STALE_AFTER_MS), Freshness::Stale);
        assert_eq!(p.freshness(1_000 + DROP_AFTER_MS), Freshness::Lost);
    }

    #[test]
    fn a_disconnected_peer_is_stale_not_live() {
        let mut p = peer(8040, "Alice");
        p.connected = false;
        // Fresh data, but the socket is down: dim it rather than showing it
        // as current, since the numbers stopped moving.
        assert_eq!(p.freshness(1_000), Freshness::Stale);
    }

    #[test]
    fn roundtime_interpolates_from_the_absolute_end_stamp() {
        let mut p = peer(8040, "Alice");
        p.roundtime_end = Some(1_100);
        assert_eq!(p.roundtime_remaining(1_095), 5.0);
        // Never negative once it has elapsed.
        assert_eq!(p.roundtime_remaining(1_200), 0.0);
        // No roundtime reported reads as zero, not as unknown-time.
        p.roundtime_end = None;
        assert_eq!(p.roundtime_remaining(1_000), 0.0);
    }
}
