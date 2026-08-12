//! Group roster tracking, parsed from the game's group messaging.
//!
//! GS4 tells you the group's *shape* only through prose: "Bob joins your
//! group.", "You join Bob.", "You disband your group." The only structured
//! signal is `<indicator id='IconJOINED'>`, a bare "you are grouped" flag with
//! no roster. So membership has to be reconstructed from text.
//!
//! Ported from Lich's `lib/gemstone/group.rb`, whose patterns are battle-worn
//! and worth reusing, with three deliberate departures:
//!
//! 1. **One identity key.** Lich stores members by `exist` id but its
//!    staleness probe compares by noun, so the two disagree. Everything here
//!    keys on the exist id, with the display name carried alongside.
//! 2. **Departures are detected.** Lich has no death or disconnect pattern at
//!    all -- a member who dies or linkdeads sits in the roster until an
//!    explicit leave message that may never come. `Departure` handles it.
//! 3. **Uncertainty is explicit.** Lich's `check` re-sends `group` on any
//!    read while unconfirmed, which can spam the game. Here an unconfirmed
//!    roster is simply reported as unconfirmed and the display says so; the
//!    only automatic `group` send is one per session on connect.
//!
//! The matchers run against the flattened line text, but membership is keyed
//! off the line's `<a exist noun>` link segments, which the parser preserves
//! (see `data::widget::LinkData`). Text tells us *what happened*; links tell
//! us *to whom*.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use regex::Regex;

/// One member of the group, identified the way the game identifies people.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMember {
    /// The `exist` id from the link. Stable for the session and the only
    /// reliable identity -- two players can share a display name.
    pub id: String,
    /// The `noun` from the link (usually the first name).
    pub noun: String,
    /// The link's display text.
    pub name: String,
}

/// Who leads the group.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "who")]
pub enum GroupLeader {
    /// Not in a group at all.
    #[default]
    None,
    /// This character leads.
    SelfLed,
    /// This character follows someone else.
    Other(GroupMember),
}

/// The group roster as currently understood.
///
/// `confirmed` is the honest-uncertainty flag: false means the roster may be
/// incomplete, which happens whenever we join someone else's group (we can
/// see that we joined, but not who else is already in it). A display should
/// say "unconfirmed" rather than presenting a partial roster as fact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupState {
    pub leader: GroupLeader,
    /// Members excluding this character. Ordered by first sighting.
    pub members: Vec<GroupMember>,
    /// Whether the roster has been confirmed against a `group` reply.
    pub confirmed: bool,
    /// Bumped on every change, for delta suppression.
    pub generation: u64,
}

impl GroupState {
    /// True when this character is in a group at all.
    pub fn is_grouped(&self) -> bool {
        !matches!(self.leader, GroupLeader::None) || !self.members.is_empty()
    }

    /// True when this character leads.
    pub fn leads(&self) -> bool {
        matches!(self.leader, GroupLeader::SelfLed)
    }

    /// Everyone in the group including the leader, for display. When we
    /// follow someone, the leader is a member of the group we are in but is
    /// tracked separately, so callers that want "the whole roster" ask here.
    pub fn everyone(&self) -> Vec<&GroupMember> {
        let mut out = Vec::with_capacity(self.members.len() + 1);
        if let GroupLeader::Other(leader) = &self.leader {
            out.push(leader);
        }
        out.extend(self.members.iter());
        out
    }

    fn bump(&mut self) {
        self.generation += 1;
    }

    /// Add a member if not already present. Returns true if the roster changed.
    pub fn add(&mut self, member: GroupMember) -> bool {
        if self.members.iter().any(|m| m.id == member.id) {
            return false;
        }
        self.members.push(member);
        self.bump();
        true
    }

    /// Remove a member by exist id. Returns true if the roster changed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.members.len();
        self.members.retain(|m| m.id != id);
        // Losing the leader we follow means we are no longer in their group.
        if let GroupLeader::Other(leader) = &self.leader {
            if leader.id == id {
                self.leader = GroupLeader::None;
                self.confirmed = false;
                self.bump();
                return true;
            }
        }
        if self.members.len() != before {
            self.bump();
            return true;
        }
        false
    }

    /// Clear the roster entirely (disband, or the JOINED indicator going off).
    pub fn clear(&mut self) {
        if self.members.is_empty() && matches!(self.leader, GroupLeader::None) && self.confirmed {
            return;
        }
        self.members.clear();
        self.leader = GroupLeader::None;
        // An empty group is a known-good state, not an uncertain one.
        self.confirmed = true;
        self.bump();
    }

    /// Replace the roster wholesale from a `group` reply. This is the only
    /// path that sets `confirmed`.
    pub fn replace(&mut self, leader: GroupLeader, members: Vec<GroupMember>) {
        if self.leader != leader || self.members != members || !self.confirmed {
            self.leader = leader;
            self.members = members;
            self.confirmed = true;
            self.bump();
        }
    }

    /// Mark the roster as possibly incomplete -- we know we are in a group but
    /// not who else is in it.
    pub fn mark_unconfirmed(&mut self) {
        if self.confirmed {
            self.confirmed = false;
            self.bump();
        }
    }
}

/// What a group-related line means. Resolving a line to one of these is pure
/// text work; applying it needs the line's links, which the caller supplies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupEvent {
    /// Someone joined the group we lead ("X joins your group.", "You add X").
    /// Carries the position of the relevant link on the line.
    Joined,
    /// Someone left the group we lead.
    Left,
    /// We joined someone else's group; the first link is the new leader.
    JoinedOther,
    /// We disbanded, were removed, or the group emptied.
    Disbanded,
    /// Leadership passed to us.
    GainedLeadership,
    /// The start of a `group` reply naming the whole roster. `self_led` is
    /// true for "You are leading", false for "You are grouped with".
    RosterLine { self_led: bool },
    /// "You are not currently in a group."
    NoGroup,
    /// The `group` reply's final line -- the completion sentinel.
    StatusLine,
    /// A member is gone for a reason the game does not announce as a leave:
    /// death or disconnect. Lich has no equivalent; without it the roster
    /// silently keeps a corpse.
    Departure,
}

// Lich matches these against raw XML because it has no parsed link data. We
// match the flattened text and read identity from the parsed links instead,
// so the patterns are the same sentences with the markup removed.

static RE_JOINS_YOUR_GROUP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+) joins your group\.$").unwrap());
static RE_LEAVES_YOUR_GROUP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+) leaves your group\.$").unwrap());
static RE_YOU_ADD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^You add (.+) to your group\.$").unwrap());
static RE_YOU_REMOVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^You remove (.+) from the group\.$").unwrap());
static RE_YOU_JOIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^You join (.+)\.$").unwrap());
static RE_ADDS_YOU: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+) adds you to (.+) group\.$").unwrap());
static RE_OTHER_JOINS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+) joins (.+) group\.$").unwrap());
static RE_LEADER_ADDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+) adds (.+) to (.+) group\.$").unwrap());
static RE_LEADER_REMOVES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+) removes (.+) from the group\.$").unwrap());
static RE_ROSTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^You are (leading|grouped with) ").unwrap());
static RE_STATUS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Your group status is currently (open|closed)\.$").unwrap());

/// Cheap gate so ordinary lines pay almost nothing. Every pattern above
/// contains one of these substrings -- note "You are leading X." carries
/// none of the obvious ones, which is exactly the kind of miss a narrow gate
/// makes silently.
fn might_be_group_line(line: &str) -> bool {
    line.contains("group")
        || line.contains("You join ")
        || line.starts_with("You are leading ")
        || line.contains("designates you as the new leader")
}

/// Classify every group event in a flushed chunk.
///
/// A single flush can carry SEVERAL game lines: nothing flushes on an
/// embedded newline, so a server burst like "Bob joins your group.\r\nCarol
/// joins your group.\r\n" arrives as one string. Splitting here is what makes
/// a multi-event burst work -- matching the whole blob against `$`-anchored
/// patterns finds nothing.
///
/// Returns one event per matching line, in order, since a `group` reply's
/// roster line and its status sentinel can share a flush and must stay
/// sequenced.
pub fn classify_chunk(chunk: &str) -> Vec<GroupEvent> {
    chunk.lines().filter_map(classify_line).collect()
}

/// Classify a single game line as a group event, if it is one.
///
/// Order matters: the more specific three-party forms ("X adds Y to Z group")
/// must be tested before the two-party ones they would otherwise match.
///
/// Trailing whitespace is trimmed here rather than at every call site, since
/// these patterns are `$`-anchored and the feed carries "\r\n".
pub fn classify_line(line: &str) -> Option<GroupEvent> {
    let line = line.trim_end();
    if !might_be_group_line(line) {
        return None;
    }

    if RE_STATUS.is_match(line) {
        return Some(GroupEvent::StatusLine);
    }
    if line.starts_with("You are not currently in a group") {
        return Some(GroupEvent::NoGroup);
    }
    if let Some(c) = RE_ROSTER.captures(line) {
        return Some(GroupEvent::RosterLine {
            self_led: &c[1] == "leading",
        });
    }
    if line.starts_with("You disband your group") {
        return Some(GroupEvent::Disbanded);
    }
    if line.contains("designates you as the new leader of the group") {
        return Some(GroupEvent::GainedLeadership);
    }

    // Anything about US must be tested before the general third-party forms,
    // which are looser and would otherwise swallow it: "Bob adds you to Bob's
    // group" also matches the shape of "X adds Y to Z group".
    if RE_ADDS_YOU.is_match(line) {
        return Some(GroupEvent::JoinedOther);
    }
    if RE_YOU_JOIN.is_match(line) {
        return Some(GroupEvent::JoinedOther);
    }

    // Now the third-party forms: someone else's group changing while we watch.
    if RE_LEADER_ADDS.is_match(line) {
        return Some(GroupEvent::Joined);
    }
    if RE_LEADER_REMOVES.is_match(line) {
        return Some(GroupEvent::Left);
    }
    if RE_OTHER_JOINS.is_match(line) {
        return Some(GroupEvent::Joined);
    }

    if RE_JOINS_YOUR_GROUP.is_match(line) || RE_YOU_ADD.is_match(line) {
        return Some(GroupEvent::Joined);
    }
    if RE_LEAVES_YOUR_GROUP.is_match(line) || RE_YOU_REMOVE.is_match(line) {
        return Some(GroupEvent::Left);
    }

    None
}

/// Apply one classified event, with the links found on the same line, to the
/// roster.
///
/// `roster_pending` is the caller's cursor into a multi-line `group` reply:
/// the roster line names everyone, and the status sentinel that follows ends
/// the reply. It is threaded through rather than stored on `GroupState` so a
/// half-read reply cannot leak into the observable roster.
pub fn apply_event(
    state: &mut GroupState,
    event: &GroupEvent,
    members: &[GroupMember],
    roster_pending: &mut Option<(GroupLeader, Vec<GroupMember>)>,
) {
    match event {
        GroupEvent::Joined => {
            // Third-party joins ("Carol joins Bob's group") carry links for
            // people who are not in OUR group. Only take them when we lead --
            // if we follow, we cannot see the full roster anyway, so the
            // honest move is to flag uncertainty rather than guess.
            if state.leads() {
                for m in members {
                    state.add(m.clone());
                }
            } else if state.is_grouped() {
                state.mark_unconfirmed();
            }
        }
        GroupEvent::Left => {
            for m in members {
                state.remove(&m.id);
            }
        }
        GroupEvent::JoinedOther => {
            // We joined someone else's group. The first link is the leader.
            // We can see that we joined but NOT who else is already in it,
            // so the roster is explicitly unconfirmed until a `group` reply.
            if let Some(leader) = members.first() {
                state.leader = GroupLeader::Other(leader.clone());
                state.members.clear();
                state.confirmed = false;
                state.generation += 1;
            }
        }
        GroupEvent::Disbanded | GroupEvent::NoGroup => state.clear(),
        GroupEvent::GainedLeadership => {
            // Leadership passed to us; the roster carries over.
            if !state.leads() {
                state.leader = GroupLeader::SelfLed;
                // We inherit a group whose full membership we may not have
                // seen, so this is not a confirmed roster.
                state.confirmed = false;
                state.generation += 1;
            }
        }
        GroupEvent::RosterLine { self_led } => {
            // Start of a `group` reply. When we follow, the first link is the
            // leader and the rest are members; when we lead, all links are
            // members. Held aside until the status sentinel confirms the
            // reply completed -- a truncated reply must not clobber a good
            // roster.
            let (leader, rest) = if *self_led {
                (GroupLeader::SelfLed, members.to_vec())
            } else {
                match members.split_first() {
                    Some((first, rest)) => {
                        (GroupLeader::Other(first.clone()), rest.to_vec())
                    }
                    None => (GroupLeader::None, Vec::new()),
                }
            };
            *roster_pending = Some((leader, rest));
        }
        GroupEvent::StatusLine => {
            // The reply's final line. Commit whatever the roster line staged.
            if let Some((leader, members)) = roster_pending.take() {
                state.replace(leader, members);
            } else {
                // A status line with no preceding roster line means the reply
                // said we are in no group; `NoGroup` already cleared it. Mark
                // confirmed so the display stops hedging.
                state.confirmed = true;
            }
        }
        GroupEvent::Departure => {
            for m in members {
                state.remove(&m.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, name: &str) -> GroupMember {
        GroupMember {
            id: id.to_string(),
            noun: name.to_ascii_lowercase(),
            name: name.to_string(),
        }
    }

    #[test]
    fn classifies_joins_and_leaves_of_our_own_group() {
        assert_eq!(
            classify_line("Bob joins your group."),
            Some(GroupEvent::Joined)
        );
        assert_eq!(
            classify_line("You add Bob to your group."),
            Some(GroupEvent::Joined)
        );
        assert_eq!(
            classify_line("Bob leaves your group."),
            Some(GroupEvent::Left)
        );
        assert_eq!(
            classify_line("You remove Bob from the group."),
            Some(GroupEvent::Left)
        );
    }

    #[test]
    fn classifies_joining_someone_elses_group() {
        assert_eq!(
            classify_line("You join Bob."),
            Some(GroupEvent::JoinedOther)
        );
        assert_eq!(
            classify_line("Bob adds you to Bob's group."),
            Some(GroupEvent::JoinedOther)
        );
    }

    #[test]
    fn classifies_third_party_membership_changes() {
        // Someone else's group changing while we watch. These must not be
        // mistaken for changes to OUR group.
        assert_eq!(
            classify_line("Bob adds Carol to Bob's group."),
            Some(GroupEvent::Joined)
        );
        assert_eq!(
            classify_line("Bob removes Carol from the group."),
            Some(GroupEvent::Left)
        );
        assert_eq!(
            classify_line("Carol joins Bob's group."),
            Some(GroupEvent::Joined)
        );
    }

    #[test]
    fn classifies_roster_reply_lines() {
        assert_eq!(
            classify_line("You are leading Bob."),
            Some(GroupEvent::RosterLine { self_led: true })
        );
        assert_eq!(
            classify_line("You are grouped with Bob."),
            Some(GroupEvent::RosterLine { self_led: false })
        );
        assert_eq!(
            classify_line("You are not currently in a group."),
            Some(GroupEvent::NoGroup)
        );
        assert_eq!(
            classify_line("Your group status is currently open."),
            Some(GroupEvent::StatusLine)
        );
        assert_eq!(
            classify_line("Your group status is currently closed."),
            Some(GroupEvent::StatusLine)
        );
    }

    #[test]
    fn classifies_disband_and_leadership() {
        assert_eq!(
            classify_line("You disband your group."),
            Some(GroupEvent::Disbanded)
        );
        assert_eq!(
            classify_line("Bob designates you as the new leader of the group."),
            Some(GroupEvent::GainedLeadership)
        );
    }

    #[test]
    fn classify_chunk_splits_a_multi_line_burst() {
        // One flush routinely carries several game lines. Matching the whole
        // blob against `$`-anchored patterns finds nothing, so the chunk must
        // be split first -- this is the shape that shipped broken until an
        // end-to-end test caught it.
        let chunk = "Bob joins your group.\r\nCarol joins your group.\r\n";
        assert_eq!(
            classify_chunk(chunk),
            vec![GroupEvent::Joined, GroupEvent::Joined]
        );

        // A `group` reply's roster line and sentinel share a flush and must
        // stay in order.
        let reply = "You are leading Bob.\r\nYour group status is currently open.\r\n";
        assert_eq!(
            classify_chunk(reply),
            vec![
                GroupEvent::RosterLine { self_led: true },
                GroupEvent::StatusLine
            ]
        );

        // Non-group lines interleaved with group lines are skipped, not
        // treated as separators.
        let mixed = "You see nothing unusual.\r\nBob joins your group.\r\n";
        assert_eq!(classify_chunk(mixed), vec![GroupEvent::Joined]);
    }

    #[test]
    fn ignores_ordinary_lines() {
        // The gate must be cheap AND not fire on unrelated prose. "group" in
        // an unrelated sentence must not produce an event.
        assert_eq!(classify_line("You see nothing unusual."), None);
        assert_eq!(classify_line("A group of tourists wanders past."), None);
        assert_eq!(classify_line("The sky is blue."), None);
        assert_eq!(classify_line(""), None);
    }

    #[test]
    fn roster_add_is_idempotent_by_id() {
        let mut g = GroupState::default();
        assert!(g.add(member("-1", "Bob")));
        // Same id again: no change, even though a fresh struct is passed.
        assert!(!g.add(member("-1", "Bob")));
        assert_eq!(g.members.len(), 1);

        // Same NAME but a different id is a different person, and both stay.
        assert!(g.add(member("-2", "Bob")));
        assert_eq!(g.members.len(), 2);
    }

    #[test]
    fn removing_the_leader_we_follow_ends_the_group() {
        let mut g = GroupState::default();
        g.replace(GroupLeader::Other(member("-1", "Bob")), vec![member("-2", "Carol")]);
        assert!(g.is_grouped());

        // Bob dies or leaves: we are no longer in his group, and what remains
        // is uncertain -- Carol may or may not still be grouped with us.
        assert!(g.remove("-1"));
        assert_eq!(g.leader, GroupLeader::None);
        assert!(!g.confirmed);
    }

    #[test]
    fn clear_is_a_confirmed_empty_state() {
        let mut g = GroupState::default();
        g.replace(GroupLeader::SelfLed, vec![member("-1", "Bob")]);
        g.clear();
        assert!(!g.is_grouped());
        assert!(g.members.is_empty());
        // Disbanding leaves NO doubt about the roster, unlike joining someone
        // else's group. Confirmed, not unconfirmed.
        assert!(g.confirmed);
    }

    #[test]
    fn everyone_includes_the_leader_we_follow() {
        let mut g = GroupState::default();
        g.replace(GroupLeader::Other(member("-1", "Bob")), vec![member("-2", "Carol")]);
        let names: Vec<&str> = g.everyone().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["Bob", "Carol"]);

        // When we lead, there is no separate leader entry to prepend.
        let mut g2 = GroupState::default();
        g2.replace(GroupLeader::SelfLed, vec![member("-2", "Carol")]);
        let names: Vec<&str> = g2.everyone().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["Carol"]);
    }

    #[test]
    fn generation_only_bumps_on_real_change() {
        let mut g = GroupState::default();
        g.add(member("-1", "Bob"));
        let gen = g.generation;
        g.add(member("-1", "Bob"));
        assert_eq!(g.generation, gen, "no-op add must not bump");
        g.remove("-999");
        assert_eq!(g.generation, gen, "removing a non-member must not bump");
    }

    /// Drive a sequence of (event, links) through `apply_event` the way the
    /// prompt drain does, and hand back the resulting roster.
    fn drive(events: &[(GroupEvent, Vec<GroupMember>)]) -> GroupState {
        let mut state = GroupState::default();
        let mut pending = None;
        for (event, members) in events {
            apply_event(&mut state, event, members, &mut pending);
        }
        state
    }

    #[test]
    fn group_reply_commits_only_on_the_status_sentinel() {
        // "You are leading Bob, Carol." then "Your group status is ..."
        let g = drive(&[
            (
                GroupEvent::RosterLine { self_led: true },
                vec![member("-1", "Bob"), member("-2", "Carol")],
            ),
            (GroupEvent::StatusLine, vec![]),
        ]);
        assert_eq!(g.leader, GroupLeader::SelfLed);
        assert_eq!(g.members.len(), 2);
        assert!(g.confirmed);
    }

    #[test]
    fn truncated_group_reply_does_not_clobber_the_roster() {
        // A roster line with no status sentinel (reply cut off, or split
        // across prompts) must leave the previous roster intact rather than
        // committing a partial one.
        let mut state = GroupState::default();
        let mut pending = None;
        apply_event(
            &mut state,
            &GroupEvent::RosterLine { self_led: true },
            &[member("-1", "Bob")],
            &mut pending,
        );
        // Staged, not applied.
        assert!(state.members.is_empty());
        assert!(!state.confirmed);
    }

    #[test]
    fn following_roster_line_splits_leader_from_members() {
        // "You are grouped with Bob, Carol." -- Bob leads, Carol is a peer.
        let g = drive(&[
            (
                GroupEvent::RosterLine { self_led: false },
                vec![member("-1", "Bob"), member("-2", "Carol")],
            ),
            (GroupEvent::StatusLine, vec![]),
        ]);
        assert_eq!(g.leader, GroupLeader::Other(member("-1", "Bob")));
        assert_eq!(g.members, vec![member("-2", "Carol")]);
        assert!(g.confirmed);
    }

    #[test]
    fn joining_someone_elses_group_is_unconfirmed() {
        // We can see we joined, but not who else is already in Bob's group.
        // Claiming an empty roster would be a lie.
        let g = drive(&[(GroupEvent::JoinedOther, vec![member("-1", "Bob")])]);
        assert_eq!(g.leader, GroupLeader::Other(member("-1", "Bob")));
        assert!(!g.confirmed, "roster is not knowable from the join alone");
        assert!(g.is_grouped());
    }

    #[test]
    fn third_party_joins_do_not_pollute_our_roster_when_following() {
        // "Carol joins Bob's group" while we merely follow Bob. We cannot
        // tell whose group changed, so we flag uncertainty rather than
        // inventing a member.
        let mut state = GroupState::default();
        let mut pending = None;
        apply_event(
            &mut state,
            &GroupEvent::JoinedOther,
            &[member("-1", "Bob")],
            &mut pending,
        );
        apply_event(
            &mut state,
            &GroupEvent::Joined,
            &[member("-9", "Stranger")],
            &mut pending,
        );
        assert!(
            !state.members.iter().any(|m| m.id == "-9"),
            "a stranger must not be added to a group we do not lead"
        );
        assert!(!state.confirmed);
    }

    #[test]
    fn joins_land_in_our_roster_when_we_lead() {
        let mut state = GroupState::default();
        let mut pending = None;
        state.replace(GroupLeader::SelfLed, vec![]);
        apply_event(
            &mut state,
            &GroupEvent::Joined,
            &[member("-1", "Bob")],
            &mut pending,
        );
        assert_eq!(state.members, vec![member("-1", "Bob")]);
    }

    #[test]
    fn departure_removes_without_a_leave_message() {
        // Death and linkdeath produce no leave line. Lich has no pattern for
        // this at all and keeps a stale member forever; we remove explicitly.
        let mut state = GroupState::default();
        let mut pending = None;
        state.replace(
            GroupLeader::SelfLed,
            vec![member("-1", "Bob"), member("-2", "Carol")],
        );
        apply_event(
            &mut state,
            &GroupEvent::Departure,
            &[member("-1", "Bob")],
            &mut pending,
        );
        assert_eq!(state.members, vec![member("-2", "Carol")]);
    }

    #[test]
    fn disband_and_no_group_both_clear() {
        for event in [GroupEvent::Disbanded, GroupEvent::NoGroup] {
            let mut state = GroupState::default();
            let mut pending = None;
            state.replace(GroupLeader::SelfLed, vec![member("-1", "Bob")]);
            apply_event(&mut state, &event, &[], &mut pending);
            assert!(!state.is_grouped(), "{event:?} must clear the roster");
            assert!(state.confirmed, "{event:?} leaves no doubt");
        }
    }

    #[test]
    fn gaining_leadership_keeps_members_but_flags_uncertainty() {
        let mut state = GroupState::default();
        let mut pending = None;
        apply_event(
            &mut state,
            &GroupEvent::JoinedOther,
            &[member("-1", "Bob")],
            &mut pending,
        );
        apply_event(&mut state, &GroupEvent::GainedLeadership, &[], &mut pending);
        assert!(state.leads());
        // We inherited a group whose full membership we never saw.
        assert!(!state.confirmed);
    }

    #[test]
    fn mark_unconfirmed_reflects_joining_a_group_blind() {
        let mut g = GroupState::default();
        g.replace(GroupLeader::SelfLed, vec![]);
        assert!(g.confirmed);
        // We just joined someone else's group: we saw the join, but not the
        // roster. Saying "you are alone with Bob" would be a guess.
        g.mark_unconfirmed();
        assert!(!g.confirmed);
    }
}
