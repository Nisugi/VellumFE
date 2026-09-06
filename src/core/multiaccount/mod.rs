//! Multi-account status: what every character on this machine is doing, in
//! one place.
//!
//! Each running VellumFE already publishes its own state over the web
//! sidecar's websocket and registers itself in the machine-local Vellum
//! runtime registry.
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

/// How long a DISCONNECTED peer keeps its dimmed card before being dropped.
/// Lich's groupbar uses the same two-stage idea; a brief socket blip should
/// dim the card, not blank it.
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

    /// Active effects by category ("ActiveSpells", "Buffs", "Debuffs",
    /// "Cooldowns"). Entries carry an absolute `expires_at`, so a card counts
    /// down locally rather than the peer streaming ticks.
    pub effects: HashMap<String, crate::data::ActiveEffectsContent>,
    /// What the character is holding, and what they are preparing.
    /// Absolute vitals by id ("health" -> (current, max)). Empty until the
    /// peer's minivitals dialog reports; percentages in `vitals` always work.
    pub minivitals: BTreeMap<String, (u32, u32)>,
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
    pub prepared_spell: Option<String>,

    pub mind: Option<Gauge>,
    pub encumbrance: Option<Gauge>,
    pub stance: Option<Gauge>,
    /// Unabsorbed field experience as (current, max). "How close to capped",
    /// which is what decides whether a character should go absorb.
    pub field_exp: Option<(u64, u64)>,

    pub room_name: Option<String>,
    pub room_id: Option<String>,

    /// Absolute server timestamps, exactly as the feed gives them, so the
    /// display interpolates locally instead of receiving a tick per second.
    pub roundtime_end: Option<i64>,
    pub casttime_end: Option<i64>,
    pub server_time: i64,
}

/// The port a self-card is filed under.
///
/// Our own status never arrives over a socket, so it has no real port. 0 is
/// never a bound port, which makes it a safe key that also sorts first --
/// exactly where the self card belongs.
pub const SELF_PORT: u16 = 0;

impl PeerStatus {
    /// Build the local character's card from game state.
    ///
    /// The hub deliberately never dials our own instance, so this is the only
    /// source for our own card -- read from memory, not a loopback socket.
    /// Two inputs cannot come from `GameState` and must be passed in:
    /// `configured` because `character_name` only arrives via the feed's
    /// `<app>` tag (absent through Lich; the OS title bar falls back to the
    /// configured name for the same reason), and `room_id` because the real
    /// id lives on `AppCore` (nav/Lich overlay) -- `GameState.room_id` is
    /// never written. One constructor, no defaulting wrappers: the earlier
    /// three-layer chain left the two buggy narrow shapes alive as the only
    /// ones the tests exercised.
    pub fn from_local(
        game_state: &crate::core::state::GameState,
        configured: Option<&str>,
        room_id: Option<String>,
        now_ms: u64,
    ) -> Self {
        let gauge = |seen: bool, value: u32, text: &str| {
            seen.then(|| Gauge {
                value,
                text: text.to_string(),
            })
        };
        Self {
            character: game_state
                .character_name
                .clone()
                .or_else(|| configured.map(str::to_string))
                .unwrap_or_else(|| "You".to_string()),
            port: SELF_PORT,
            connected: true,
            last_update_ms: now_ms,
            vitals: game_state.vitals.clone(),
            indicators: game_state.status.clone(),
            injuries: game_state.injuries.clone(),
            group: game_state.group.clone(),
            effects: game_state.effects.clone(),
            minivitals: crate::core::remote::RemoteVital::from_state(&game_state.minivitals)
                .into_iter()
                .map(|v| (v.id, (v.value, v.max)))
                .collect(),
            left_hand: game_state.left_hand.clone(),
            right_hand: game_state.right_hand.clone(),
            prepared_spell: game_state.spell.clone(),
            mind: gauge(
                game_state.gs4_experience.generation > 0,
                game_state.gs4_experience.mind_state_value,
                &game_state.gs4_experience.mind_state_text,
            ),
            encumbrance: gauge(
                game_state.encumbrance.generation > 0,
                game_state.encumbrance.value,
                &game_state.encumbrance.text,
            ),
            stance: gauge(
                game_state.stance.generation > 0,
                game_state.stance.value,
                &game_state.stance.text,
            ),
            field_exp: match (
                game_state.gs4_experience.field_exp,
                game_state.gs4_experience.max_field_exp,
            ) {
                (Some(value), Some(max)) if max > 0 => Some((value, max)),
                _ => None,
            },
            room_name: game_state.room_name.clone(),
            room_id: room_id.or_else(|| game_state.room_id.clone()),
            roundtime_end: game_state.roundtime_end,
            casttime_end: game_state.casttime_end,
            server_time: game_state.game_time,
        }
    }

    /// Whether this card is the local character rather than a peer.
    pub fn is_self(&self) -> bool {
        self.port == SELF_PORT
    }
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
        // A connected peer is current by definition: deltas arrive on change,
        // so silence means nothing changed, not that the data aged. Grading
        // connected peers by last_update_ms reaped an AFK character's card
        // two minutes into standing still -- permanently, since discovery
        // never respawned the port.
        if self.connected {
            return Freshness::Live;
        }
        // Disconnected: last_update_ms is stamped at disconnect, so this
        // measures time since the drop, not time since the last delta.
        let age = now_ms.saturating_sub(self.last_update_ms);
        if age >= DROP_AFTER_MS {
            Freshness::Lost
        } else {
            Freshness::Stale
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
    /// The game says these characters are grouped even though we have no
    /// roster naming anyone. Distinguishes "grouped, members unknown" from
    /// "genuinely alone" -- both of which otherwise look like a nameless
    /// single-member cluster.
    pub grouped: bool,
}

impl Cluster {
    fn solo(port: u16) -> Self {
        Self {
            members: vec![port],
            leader: None,
            leader_name: None,
            confirmed: true,
            grouped: false,
        }
    }

    /// A single character who is not in a group at all. A lone character who
    /// IS grouped (with someone not ours, or with a roster we have not
    /// parsed) is not solo -- drawing them as such would contradict the game.
    pub fn is_solo(&self) -> bool {
        self.members.len() == 1 && self.leader_name.is_none() && !self.grouped
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

    // Last resort: characters the game says are grouped (the JOINED
    // indicator) but whose roster we have not parsed. Being ADDED to a group
    // produces the indicator with no message naming anyone, so the roster
    // stays empty until a `group` reply -- and without this they would each
    // render as solo while the game plainly says otherwise.
    //
    // Only used for peers with NO roster at all; a parsed roster always wins.
    // The resulting cluster is marked unconfirmed, so the display says it is
    // inferred rather than known.
    let joined_unknown: Vec<u16> = peers
        .iter()
        .filter(|(_, p)| {
            matches!(p.group.leader, GroupLeader::Unknown) && p.group.members.is_empty()
        })
        .map(|(port, _)| *port)
        .collect();
    for pair in joined_unknown.windows(2) {
        union(&mut parent, pair[0], pair[1]);
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
                match &peer.group.leader {
                    GroupLeader::Other(leader) => {
                        return Cluster {
                            members,
                            leader: None,
                            leader_name: Some(leader.name.clone()),
                            confirmed: peer.group.confirmed,
                            grouped: true,
                        };
                    }
                    // Grouped per the game, roster not yet known. Not solo --
                    // saying "alone" would contradict the indicator.
                    GroupLeader::Unknown => {
                        return Cluster {
                            members,
                            leader: None,
                            leader_name: None,
                            confirmed: false,
                            grouped: true,
                        };
                    }
                    _ => {}
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
                grouped: true,
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
    fn the_self_card_sorts_first() {
        // SELF_PORT is 0, below any bound port, so the self card leads its
        // cluster AND its cluster leads the list. That fixed position is what
        // makes it a reference point rather than something to hunt for.
        let me = peer(SELF_PORT, "Ultz");
        let other = peer(8041, "Abem");

        let clusters = cluster_peers(&peers(vec![other, me]));
        assert_eq!(clusters.len(), 2, "ungrouped: two solo cards");
        assert_eq!(clusters[0].members, vec![SELF_PORT]);
        assert_eq!(clusters[1].members, vec![8041]);
    }

    #[test]
    fn the_self_card_clusters_with_its_group() {
        // Self is an ordinary member for grouping purposes: when we lead, our
        // card leads the frame.
        let mut me = peer(SELF_PORT, "Ultz");
        me.group.replace(GroupLeader::SelfLed, vec![member("Abem")]);
        let mut them = peer(8041, "Abem");
        them.group
            .replace(GroupLeader::Other(member("Ultz")), vec![]);

        let clusters = cluster_peers(&peers(vec![them, me]));
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members, vec![SELF_PORT, 8041]);
        assert_eq!(clusters[0].leader, Some(SELF_PORT));
        assert_eq!(clusters[0].leader_name.as_deref(), Some("Ultz"));
    }

    #[test]
    fn is_self_distinguishes_the_local_card() {
        assert!(peer(SELF_PORT, "Ultz").is_self());
        assert!(!peer(8041, "Abem").is_self());
    }

    #[test]
    fn from_local_reads_game_state_and_leaves_unreported_gauges_unknown() {
        let mut gs = crate::core::state::GameState::new();
        gs.character_name = Some("Ultz".to_string());
        gs.vitals.health = 51;
        gs.status.set("IconSTUNNED", true);
        gs.injuries.insert("head".to_string(), 1);
        gs.stance.update(80, "defensive (80%)");

        let me = PeerStatus::from_local(&gs, None, None, 1_000);
        assert!(me.is_self());
        assert_eq!(me.character, "Ultz");
        assert_eq!(me.vitals.health, 51);
        assert!(me.indicators.stunned());
        assert_eq!(me.injuries.get("head"), Some(&1));
        assert_eq!(me.stance.as_ref().map(|g| g.value), Some(80));
        // Never reported: stays unknown rather than reading as 0%.
        assert!(me.mind.is_none());
        assert!(me.encumbrance.is_none());
        // Always current -- it is read from memory, not a socket.
        assert!(me.connected);
        assert_eq!(me.freshness(1_000), Freshness::Live);
    }

    #[test]
    fn from_local_falls_back_when_the_character_is_unnamed() {
        // Before login there is no character name; the card still needs a
        // label rather than rendering blank.
        let gs = crate::core::state::GameState::new();
        assert_eq!(PeerStatus::from_local(&gs, None, None, 0).character, "You");
    }

    /// Being ADDED to a group sets the JOINED indicator with no message
    /// naming anyone, so the roster stays empty. Two of our characters in
    /// that state are grouped as far as the game is concerned, and must not
    /// each render as solo.
    #[test]
    fn joined_without_a_roster_still_clusters() {
        let mut a = peer(8040, "Abem");
        a.group.mark_joined_unconfirmed();
        let mut b = peer(8041, "Ultz");
        b.group.mark_joined_unconfirmed();

        let clusters = cluster_peers(&peers(vec![a, b]));
        assert_eq!(clusters.len(), 1, "the game says they are grouped");
        assert_eq!(clusters[0].members, vec![8040, 8041]);
        assert!(
            !clusters[0].confirmed,
            "inferred from the indicator, not a parsed roster"
        );
    }

    #[test]
    fn a_lone_joined_character_is_not_solo() {
        let mut a = peer(8040, "Abem");
        a.group.mark_joined_unconfirmed();
        let clusters = cluster_peers(&peers(vec![a]));
        assert!(
            !clusters[0].is_solo(),
            "grouped with someone, just not one of ours"
        );
    }

    /// A parsed roster always beats the indicator guess.
    #[test]
    fn a_real_roster_wins_over_the_joined_fallback() {
        let mut a = peer(8040, "Abem");
        a.group.replace(GroupLeader::SelfLed, vec![member("Ultz")]);
        let mut b = peer(8041, "Ultz");
        b.group.replace(GroupLeader::Other(member("Abem")), vec![]);
        // A third character grouped with strangers must not be pulled in.
        let mut c = peer(8042, "Stranger");
        c.group.mark_joined_unconfirmed();

        let clusters = cluster_peers(&peers(vec![a, b, c]));
        assert_eq!(clusters.len(), 2, "{clusters:?}");
        assert_eq!(clusters[0].members, vec![8040, 8041]);
        assert!(clusters[0].confirmed);
        assert_eq!(clusters[1].members, vec![8042]);
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
        a.group
            .replace(GroupLeader::SelfLed, vec![member("Bob"), member("Carol")]);
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
        f.group.replace(GroupLeader::Other(member("Eve")), vec![]);

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
        b.group.replace(GroupLeader::Other(member("Alice")), vec![]);
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
        b.group.replace(GroupLeader::Other(member("Alice")), vec![]);
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
    fn a_connected_peer_is_live_no_matter_how_quiet() {
        // An AFK character emits no deltas for hours; its data is still
        // current. The old age-based grading reaped exactly this card.
        let p = peer(8040, "Alice"); // connected, last_update_ms = 1_000
        assert_eq!(p.freshness(1_000), Freshness::Live);
        assert_eq!(p.freshness(1_000 + DROP_AFTER_MS * 10), Freshness::Live);
    }

    #[test]
    fn a_disconnected_peer_dims_then_drops() {
        let mut p = peer(8040, "Alice");
        p.connected = false;
        p.last_update_ms = 1_000; // stamped at disconnect
        assert_eq!(p.freshness(1_000), Freshness::Stale);
        assert_eq!(p.freshness(1_000 + DROP_AFTER_MS - 1), Freshness::Stale);
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
